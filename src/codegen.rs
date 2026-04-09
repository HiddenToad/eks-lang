use crate::parse::*;
use crate::Type;
use inkwell::{
    builder::Builder,
    context::Context,
    module::Module,
    types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, StructType},
    values::{BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, PointerValue},
    AddressSpace,
};
use std::collections::{HashMap, HashSet};

struct QueryVar<'ctx> {
    comp_arrays: HashMap<String, PointerValue<'ctx>>,
    index_ptr: PointerValue<'ctx>,
    count_ptr: PointerValue<'ctx>,
}

pub struct Codegen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,

    comp_types: HashMap<String, StructType<'ctx>>,
    comp_decls: HashMap<String, CompDecl>,
    archetypes: HashMap<String, Vec<String>>,
    functions: HashMap<String, FunctionValue<'ctx>>,
    variables: HashMap<String, PointerValue<'ctx>>,
    active_query: HashMap<String, QueryVar<'ctx>>,
}

impl<'ctx> Codegen<'ctx> {
    pub fn new(context: &'ctx Context) -> Self {
        let module = context.create_module("ecs_lang");
        let builder = context.create_builder();

        Self {
            context,
            module,
            builder,
            comp_types: HashMap::new(),
            comp_decls: HashMap::new(),
            archetypes: HashMap::new(),
            functions: HashMap::new(),
            variables: HashMap::new(),
            active_query: HashMap::new(),
        }
    }

    pub fn compile(&mut self, program: &Program) -> Result<(), String> {
        //1st pass: just read decls and register names
        //This ensures that by the time we're compiling actual
        //statements, we know all the user definitions already.
        for decl in &program.decls {
            match decl {
                Decl::Comp(comp) => self.register_comp(comp)?,
                Decl::Ent(ent) => self.register_ent(ent)?,
                _ => {}
            }
        }

        //2nd pass: now that we've registered all the 
        //comps and ents, we can compile the actual behavior
        //of the program. 
        for decl in &program.decls {
            match decl {
                Decl::Let(let_decl) => self.compile_global_let(let_decl)?,
                Decl::Fun(fun_decl) => self.compile_fun(fun_decl)?,
                Decl::Sys(sys_decl) => self.compile_sys(sys_decl)?,
                _ => {}
            }
        }

        Ok(())
    }
    
    //optimization function. this makes IR generation
    //avoid passing along unused components. If an entity
    //with 5 comps is queries, but only 2 of those are read or written
    //in the body of the sys, the other 3 will never be passed
    //or iterated, saving memory and time.
    fn extract_required_components(
        &self,
        body: &[Stmt],
        query_var_name: &str,
        ent_comps: &[String],
    ) -> HashSet<String> {
        let mut required = HashSet::new();

        fn walk_expr(expr: &Expr, var_name: &str, ent_comps: &[String], out: &mut HashSet<String>) {
            match expr {
                Expr::FieldAccess { object, field } => {
                    if let Expr::Ident(ident) = object.as_ref() {
                        if ident == var_name {
                            for c in ent_comps {
                                if c.to_lowercase() == field.to_lowercase() {
                                    out.insert(c.clone());
                                }
                            }
                            return;
                        }
                    }

                    walk_expr(object, var_name, ent_comps, out);
                }
                Expr::Binary { left, right, .. } => {
                    walk_expr(left, var_name, ent_comps, out);
                    walk_expr(right, var_name, ent_comps, out);
                }
                Expr::Unary { expr, .. } => walk_expr(expr, var_name, ent_comps, out),
                Expr::Call { args, .. } => {
                    for arg in args {
                        walk_expr(arg, var_name, ent_comps, out);
                    }
                }
                _ => {}
            }
        }

        fn walk_lvalue(
            lvalue: &LValue,
            var_name: &str,
            ent_comps: &[String],
            out: &mut HashSet<String>,
        ) {
            if let LValue::FieldAccess { object, field } = lvalue {
                if let LValue::Ident(ident) = object.as_ref() {
                    if ident == var_name {
                        for c in ent_comps {
                            if c.to_lowercase() == field.to_lowercase() {
                                out.insert(c.clone());
                            }
                        }
                        return;
                    }
                }

                walk_lvalue(object, var_name, ent_comps, out);
            }
        }

        fn walk_stmt(stmt: &Stmt, var_name: &str, ent_comps: &[String], out: &mut HashSet<String>) {
            match stmt {
                Stmt::Let { value, .. } => walk_expr(value, var_name, ent_comps, out),
                Stmt::Assign { target, value } => {
                    walk_lvalue(target, var_name, ent_comps, out);
                    walk_expr(value, var_name, ent_comps, out);
                }
                Stmt::Return(Some(expr)) => walk_expr(expr, var_name, ent_comps, out),
                Stmt::Expr(expr) => walk_expr(expr, var_name, ent_comps, out),
                _ => {}
            }
        }

        for stmt in body {
            walk_stmt(stmt, query_var_name, ent_comps, &mut required);
        }

        required
    }

    //comps are registered as LLVM structs, and can be used
    //just like a standard constructable type
    //they are also stored by the compiler for queries
    fn register_comp(&mut self, comp: &CompDecl) -> Result<(), String> {
        let mut field_types = vec![];
        for field in &comp.fields {
            field_types.push(self.resolve_type(&field.ty)?);
        }
        let struct_type = self.context.opaque_struct_type(&comp.name);
        struct_type.set_body(&field_types, false);

        self.comp_types.insert(comp.name.clone(), struct_type);
        self.comp_decls.insert(comp.name.clone(), comp.clone());
        Ok(())
    }

    //ents actually don't directly emit codegen,
    //they're just stored as shortcuts for comp lists.
    //eventually, ents will be spawnable as well.
    fn register_ent(&mut self, ent: &EntDecl) -> Result<(), String> {
        for comp_name in &ent.comps {
            if !self.comp_types.contains_key(comp_name) {
                return Err(format!(
                    "Entity {} uses unknown component {}",
                    ent.name, comp_name
                ));
            }
        }
        self.archetypes.insert(ent.name.clone(), ent.comps.clone());
        Ok(())
    }

    //convert an Eks type into an LLVM type 
    fn resolve_type(&self, ty: &Type) -> Result<BasicTypeEnum<'ctx>, String> {
        match ty {
            Type::Int => Ok(self.context.i64_type().into()),
            Type::Float => Ok(self.context.f64_type().into()),
            Type::Bool => Ok(self.context.bool_type().into()),
            Type::String => Ok(self
                .context
                .i8_type()
                .ptr_type(AddressSpace::default())
                .into()),
            Type::Void => Err("Cannot resolve void as a value type".into()),
            Type::Custom(name) => self
                .comp_types
                .get(name)
                .map(|t| (*t).into())
                .ok_or_else(|| format!("Unknown type: {}", name)),
        }
    }

    fn compile_global_let(&mut self, let_decl: &LetDecl) -> Result<(), String> {
        if let Expr::StringLiteral(s) = &let_decl.value {
            let str_val = self.context.const_string(s.as_bytes(), true);
            let global = self
                .module
                .add_global(str_val.get_type(), None, &let_decl.name);
            global.set_initializer(&str_val);
            global.set_constant(true);
            Ok(())
        } else {
            Err("Global let currently only supports string literals".into())
        }
    }

    fn compile_fun(&mut self, fun: &FunDecl) -> Result<(), String> {
        self.variables.clear();
        self.active_query.clear();

        let mut param_types: Vec<BasicTypeEnum> = vec![];
        for param in &fun.params {
            param_types.push(self.resolve_type(&param.ty)?);
        }

        let param_meta: Vec<BasicMetadataTypeEnum> =
            param_types.iter().map(|t| (*t).into()).collect();

        let fn_type = match &fun.ret {
            Type::Void => self.context.void_type().fn_type(&param_meta, false),
            _ => self.resolve_type(&fun.ret)?.fn_type(&param_meta, false),
        };

        let function = self.module.add_function(&fun.name, fn_type, None);
        self.functions.insert(fun.name.clone(), function);

        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        for (i, param) in fun.params.iter().enumerate() {
            let alloca = self
                .builder
                .build_alloca(param_types[i], &param.name)
                .map_err(|e| e.to_string())?;
            self.builder
                .build_store(alloca, function.get_nth_param(i as u32).unwrap())
                .map_err(|e| e.to_string())?;
            self.variables.insert(param.name.clone(), alloca);
        }

        for stmt in &fun.body {
            self.compile_stmt(stmt)?;
        }

        if function
            .get_last_basic_block()
            .unwrap()
            .get_terminator()
            .is_none()
        {
            self.builder.build_return(None).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    
    fn compile_sys(&mut self, sys: &SysDecl) -> Result<(), String> {
        self.variables.clear();
        self.active_query.clear();

        let mut param_types: Vec<BasicTypeEnum> = vec![];
        let mut binding_expansions: Vec<(String, Vec<String>)> = vec![];

        for (var_name, queried_types) in &sys.query.bindings {
            let mut ent_comps = Vec::new();
            for t in queried_types {
                if let Some(comps) = self.archetypes.get(t) { //if any comps are entity names
                    ent_comps.extend(comps.iter().cloned()); //expand entity name into inner comps
                } else {
                    ent_comps.push(t.clone()); //otherwise just use
                }
            }

            let required_comps = self.extract_required_components(&sys.body, var_name, &ent_comps);
            if required_comps.is_empty() {
                return Err(format!(
                    "System {} queries {} but doesn't access any components",
                    sys.name, var_name
                ));
            }

            for comp_name in &required_comps {
                let comp_type = self
                    .comp_types
                    .get(comp_name)
                    .ok_or(format!("Unknown component {}", comp_name))?;
                param_types.push(comp_type.ptr_type(AddressSpace::default()).into());
            }
            param_types.push(self.context.i64_type().into());

            binding_expansions.push((var_name.clone(), required_comps.into_iter().collect()));
        }

        let param_meta: Vec<BasicMetadataTypeEnum> =
            param_types.iter().map(|t| (*t).into()).collect();
        let fn_type = self.context.void_type().fn_type(&param_meta, false);
        let function = self.module.add_function(&sys.name, fn_type, None);

        let entry = self.context.append_basic_block(function, "entry");
        let loop_cond = self.context.append_basic_block(function, "loop.cond");
        let loop_body = self.context.append_basic_block(function, "loop.body");
        let loop_end = self.context.append_basic_block(function, "loop.end");

        self.builder.position_at_end(entry);

        let mut param_idx = 0u32;
        for (var_name, required_comps) in &binding_expansions {
            let mut comp_arrays = HashMap::new();

            for comp_name in required_comps {
                let ptr = function
                    .get_nth_param(param_idx)
                    .ok_or("Missing parameter")?
                    .into_pointer_value();
                comp_arrays.insert(comp_name.clone(), ptr);
                param_idx += 1;
            }

            let count_val = function
                .get_nth_param(param_idx)
                .ok_or("Missing count")?
                .into_int_value();
            param_idx += 1;

            let index_alloca = self
                .builder
                .build_alloca(self.context.i64_type(), &format!("{}_idx", var_name))
                .map_err(|e| e.to_string())?;
            self.builder
                .build_store(index_alloca, self.context.i64_type().const_zero())
                .map_err(|e| e.to_string())?;

            let count_alloca = self
                .builder
                .build_alloca(self.context.i64_type(), &format!("{}_count", var_name))
                .map_err(|e| e.to_string())?;
            self.builder
                .build_store(count_alloca, count_val)
                .map_err(|e| e.to_string())?;

            self.active_query.insert(
                var_name.clone(),
                QueryVar {
                    comp_arrays,
                    index_ptr: index_alloca,
                    count_ptr: count_alloca,
                },
            );
        }

        self.builder
            .build_unconditional_branch(loop_cond)
            .map_err(|e| e.to_string())?;

        let query_var = self.active_query.values().next().unwrap();
        let index_ptr = query_var.index_ptr;
        let count_ptr = query_var.count_ptr;

        self.builder.position_at_end(loop_cond);
        let current_idx = self
            .builder
            .build_load(index_ptr, "i")
            .map_err(|e| e.to_string())?
            .into_int_value();
        let count_val = self
            .builder
            .build_load(count_ptr, "count")
            .map_err(|e| e.to_string())?
            .into_int_value();

        let is_done = self
            .builder
            .build_int_compare(inkwell::IntPredicate::ULT, current_idx, count_val, "done")
            .map_err(|e| e.to_string())?;
        self.builder
            .build_conditional_branch(is_done, loop_body, loop_end)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(loop_body);
        for stmt in &sys.body {
            self.compile_stmt(stmt)?;
        }

        let next_idx = self
            .builder
            .build_int_add(
                current_idx,
                self.context.i64_type().const_int(1, false),
                "next_i",
            )
            .map_err(|e| e.to_string())?;
        self.builder
            .build_store(index_ptr, next_idx)
            .map_err(|e| e.to_string())?;
        self.builder
            .build_unconditional_branch(loop_cond)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(loop_end);
        self.builder.build_return(None).map_err(|e| e.to_string())?;

        Ok(())
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Let { name, value } => {
                let val = self.compile_expr(value)?;
                let alloca = self
                    .builder
                    .build_alloca(val.get_type(), name)
                    .map_err(|e| e.to_string())?;
                self.builder
                    .build_store(alloca, val)
                    .map_err(|e| e.to_string())
                    .map(|_| ())
            }
            Stmt::Return(expr) => {
                let ret_val = match expr {
                    Some(e) => Some(self.compile_expr(e)?),
                    None => None,
                };
                let ret_ref = ret_val.as_ref().map(|v| v as &dyn BasicValue);
                self.builder
                    .build_return(ret_ref)
                    .map_err(|e| e.to_string())
                    .map(|_| ())
            }
            Stmt::Expr(expr) => {
                self.compile_expr(expr)?;
                Ok(())
            }
            Stmt::Assign { target, value } => {
                let ptr = self.compile_lvalue(target)?;
                let val = self.compile_expr(value)?;
                self.builder
                    .build_store(ptr, val)
                    .map_err(|e| e.to_string())
                    .map(|_| ())
            }
        }
    }
    fn compile_lvalue(&mut self, lvalue: &LValue) -> Result<PointerValue<'ctx>, String> {
        match lvalue {
            LValue::Ident(name) => self
                .variables
                .get(name)
                .cloned()
                .ok_or(format!("Unknown variable {}", name)),
            LValue::FieldAccess { object, field } => {
                if let LValue::Ident(var_name) = object.as_ref() {
                    if let Some(query_var) = self.active_query.get(var_name) {
                        let base_array_ptr = query_var
                            .comp_arrays
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case(field))
                            .map(|(_, &v)| v);

                        if let Some(base_array_ptr) = base_array_ptr {
                            let current_idx = self
                                .builder
                                .build_load(query_var.index_ptr, "i")
                                .map_err(|e| e.to_string())?
                                .into_int_value();

                            return unsafe {
                                self.builder
                                    .build_in_bounds_gep(
                                        base_array_ptr,
                                        &[current_idx],
                                        "entity_comp_ptr",
                                    )
                                    .map_err(|e| e.to_string())
                            };
                        }
                    }
                }

                let base_ptr = self.compile_lvalue(object)?;
                let (comp_name, _) = self.find_component_for_field(field)?;
                let field_idx = self.get_field_index(&comp_name, field)?;

                self.builder
                    .build_struct_gep(base_ptr, field_idx, "field_ptr")
                    .map_err(|e| e.to_string())
            }
        }
    }

    fn compile_expr_to_ptr(&mut self, expr: &Expr) -> Result<PointerValue<'ctx>, String> {
        match expr {
            Expr::Ident(name) => self
                .variables
                .get(name)
                .cloned()
                .ok_or(format!("Unknown variable {}", name)),
            Expr::FieldAccess { object, field } => {
                if let Expr::Ident(var_name) = object.as_ref() {
                    if let Some(query_var) = self.active_query.get(var_name) {
                        let base_array_ptr = query_var
                            .comp_arrays
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case(field))
                            .map(|(_, &v)| v);

                        if let Some(base_array_ptr) = base_array_ptr {
                            let current_idx = self
                                .builder
                                .build_load(query_var.index_ptr, "i")
                                .map_err(|e| e.to_string())?
                                .into_int_value();
                            return unsafe {
                                self.builder
                                    .build_in_bounds_gep(
                                        base_array_ptr,
                                        &[current_idx],
                                        "entity_comp_ptr",
                                    )
                                    .map_err(|e| e.to_string())
                            };
                        }
                    }
                }
                let base_ptr = self.compile_expr_to_ptr(object)?;
                let (comp_name, _) = self.find_component_for_field(field)?;
                let field_idx = self.get_field_index(&comp_name, field)?;
                self.builder
                    .build_struct_gep(base_ptr, field_idx, "field_ptr")
                    .map_err(|e| e.to_string())
            }
            _ => Err("Cannot get pointer to this expression".into()),
        }
    }

    fn find_component_for_field(
        &self,
        field_name: &str,
    ) -> Result<(String, StructType<'ctx>), String> {
        for (comp_name, comp_decl) in &self.comp_decls {
            for field in &comp_decl.fields {
                if field.name == field_name {
                    if let Some(comp_type) = self.comp_types.get(comp_name) {
                        return Ok((comp_name.clone(), *comp_type));
                    }
                }
            }
        }
        Err(format!("Unknown field {}", field_name))
    }

    fn get_field_index(&self, comp_name: &str, field_name: &str) -> Result<u32, String> {
        let comp_decl = self.comp_decls.get(comp_name).ok_or("Unknown comp")?;
        for (i, field) in comp_decl.fields.iter().enumerate() {
            if field.name == field_name {
                return Ok(i as u32);
            }
        }
        Err(format!("Unknown field {} in {}", field_name, comp_name))
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<BasicValueEnum<'ctx>, String> {
        match expr {
            Expr::Int(n) => Ok(self.context.i64_type().const_int(*n as u64, false).into()),
            Expr::Float(f) => Ok(self.context.f64_type().const_float(*f).into()),
            Expr::Bool(b) => Ok(self.context.bool_type().const_int(*b as u64, false).into()),
            Expr::StringLiteral(s) => Ok(self
                .builder
                .build_global_string_ptr(s, "strtmp")
                .map_err(|e| e.to_string())?
                .as_basic_value_enum()),
            Expr::Ident(name) => {
                if let Some(qv) = self.active_query.get(name) {
                    return self
                        .builder
                        .build_load(qv.index_ptr, name)
                        .map_err(|e| e.to_string());
                }
                let ptr = self
                    .variables
                    .get(name)
                    .ok_or(format!("Unknown variable: {}", name))?;
                self.builder
                    .build_load(*ptr, name)
                    .map_err(|e| e.to_string())
            }
            Expr::Call { callee, args } => {
                if let Some(struct_type) = self.comp_types.get(callee) {
                    let comp_decl = self.comp_decls.get(callee).unwrap();

                    if args.len() != comp_decl.fields.len() {
                        return Err(format!(
                            "Component {} expects {} arguments, got {}",
                            callee,
                            comp_decl.fields.len(),
                            args.len()
                        ));
                    }

                    let alloca = self
                        .builder
                        .build_alloca(*struct_type, &format!("{}_init", callee))
                        .map_err(|e| e.to_string())?;

                    for (i, arg_expr) in args.iter().enumerate() {
                        let val = self.compile_expr(arg_expr)?;
                        let field_ptr = self
                            .builder
                            .build_struct_gep(alloca, i as u32, "init_field")
                            .map_err(|e| e.to_string())?;
                        self.builder
                            .build_store(field_ptr, val)
                            .map_err(|e| e.to_string())?;
                    }

                    return Ok(alloca.as_basic_value_enum());
                }

                let func = self
                    .functions
                    .get(callee)
                    .cloned()
                    .ok_or(format!("Unknown function or type: {}", callee))?;

                let mut compiled_args: Vec<BasicMetadataValueEnum> = vec![];
                for arg in args {
                    compiled_args.push(self.compile_expr(arg)?.into());
                }

                let call_site = self
                    .builder
                    .build_call(func, &compiled_args, "calltmp")
                    .map_err(|e| e.to_string())?;
                call_site
                    .try_as_basic_value()
                    .left()
                    .ok_or("Void function used as value".into())
            }
            Expr::Binary { left, op, right } => {
                let l = self.compile_expr(left)?.into_int_value();
                let r = self.compile_expr(right)?.into_int_value();

                let result = match op {
                    BinOp::Add => self
                        .builder
                        .build_int_add(l, r, "tmp")
                        .map_err(|e| e.to_string())?,
                    BinOp::Sub => self
                        .builder
                        .build_int_sub(l, r, "tmp")
                        .map_err(|e| e.to_string())?,
                    BinOp::Mul => self
                        .builder
                        .build_int_mul(l, r, "tmp")
                        .map_err(|e| e.to_string())?,
                    BinOp::Div => self
                        .builder
                        .build_int_signed_div(l, r, "tmp")
                        .map_err(|e| e.to_string())?,
                    BinOp::Eq => self
                        .builder
                        .build_int_compare(inkwell::IntPredicate::EQ, l, r, "tmp")
                        .map_err(|e| e.to_string())?,
                    BinOp::Neq => self
                        .builder
                        .build_int_compare(inkwell::IntPredicate::NE, l, r, "tmp")
                        .map_err(|e| e.to_string())?,
                    BinOp::Lt => self
                        .builder
                        .build_int_compare(inkwell::IntPredicate::SLT, l, r, "tmp")
                        .map_err(|e| e.to_string())?,
                    BinOp::Gt => self
                        .builder
                        .build_int_compare(inkwell::IntPredicate::SGT, l, r, "tmp")
                        .map_err(|e| e.to_string())?,
                    BinOp::Lte => self
                        .builder
                        .build_int_compare(inkwell::IntPredicate::SLE, l, r, "tmp")
                        .map_err(|e| e.to_string())?,
                    BinOp::Gte => self
                        .builder
                        .build_int_compare(inkwell::IntPredicate::SGE, l, r, "tmp")
                        .map_err(|e| e.to_string())?,
                    BinOp::And => self
                        .builder
                        .build_and(l, r, "tmp")
                        .map_err(|e| e.to_string())?,
                    BinOp::Or => self
                        .builder
                        .build_or(l, r, "tmp")
                        .map_err(|e| e.to_string())?,
                };
                Ok(result.as_basic_value_enum())
            }
            Expr::Unary { op, expr: inner } => {
                let val = self.compile_expr(inner)?.into_int_value();
                match op {
                    UnOp::Neg => Ok(self
                        .builder
                        .build_int_neg(val, "tmp")
                        .map_err(|e| e.to_string())?
                        .as_basic_value_enum()),
                    UnOp::Not => Ok(self
                        .builder
                        .build_not(val, "tmp")
                        .map_err(|e| e.to_string())?
                        .as_basic_value_enum()),
                }
            }
            Expr::FieldAccess { object, field } => {
                if let Expr::Ident(var_name) = object.as_ref() {
                    if let Some(query_var) = self.active_query.get(var_name) {
                        let base_array_ptr = query_var
                            .comp_arrays
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case(field))
                            .map(|(_, &v)| v);

                        if let Some(base_array_ptr) = base_array_ptr {
                            let current_idx = self
                                .builder
                                .build_load(query_var.index_ptr, "i")
                                .map_err(|e| e.to_string())?
                                .into_int_value();
                            let entity_comp_ptr = unsafe {
                                self.builder
                                    .build_in_bounds_gep(
                                        base_array_ptr,
                                        &[current_idx],
                                        "entity_comp_ptr",
                                    )
                                    .map_err(|e| e.to_string())?
                            };

                            let (comp_name, _) = self.find_component_for_field(field)?;
                            let field_idx = self.get_field_index(&comp_name, field)?;
                            let field_ptr = self
                                .builder
                                .build_struct_gep(entity_comp_ptr, field_idx, "field_ptr")
                                .map_err(|e| e.to_string())?;

                            return self
                                .builder
                                .build_load(field_ptr, field)
                                .map_err(|e| e.to_string());
                        }
                    }
                }

                let base_ptr = self.compile_expr_to_ptr(object)?;

                let (comp_name, _) = self.find_component_for_field(field)?;
                let field_idx = self.get_field_index(&comp_name, field)?;

                let field_ptr = self
                    .builder
                    .build_struct_gep(base_ptr, field_idx, "field_ptr")
                    .map_err(|e| e.to_string())?;

                self.builder
                    .build_load(field_ptr, field)
                    .map_err(|e| e.to_string())
            }
        }
    }

    pub fn print_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }
}
