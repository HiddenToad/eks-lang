use crate::lex::*;

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Parse error at line {}, column {}: {}",
            self.span.line, self.span.col, self.message
        )
    }
}

pub type ParseResult<T> = Result<T, ParseError>;

//Our fully parsed AST
#[derive(Debug)]
pub struct Program {
    pub decls: Vec<Decl>,
}

#[derive(Debug)]
pub enum Decl {
    Comp(CompDecl),
    Ent(EntDecl),
    Fun(FunDecl),
    Sys(SysDecl),
    Let(LetDecl),
}

//For all the Decl structs below, I give
//above an example LoC that would be parsed
//into that decl. Then, I show for each field
//which part of the example it corresponds to.

//let x = val;
#[derive(Debug)]
pub struct LetDecl {
    pub name: String, //x
    pub value: Expr,  //val
    pub span: Span,
}

//comp Foo(a: int, b: int);
#[derive(Debug, Clone)]
pub struct CompDecl {
    pub name: String,       //Foo
    pub fields: Vec<Field>, //[(a, int), (b, int)]
    pub span: Span,
}

//a: int
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String, //a
    pub ty: Type,     //int
}

//ent Baz(Foo, Bar);
#[derive(Debug)]
pub struct EntDecl {
    pub name: String,       //Baz
    pub comps: Vec<String>, //[Foo, Bar]
    pub span: Span,
}

//fun(int) truncate(a: float){
//  return int(a)
//}
#[derive(Debug)]
pub struct FunDecl {
    pub name: String,       //truncate
    pub params: Vec<Param>, //[(a, float)]
    pub ret: Type,          //int
    pub body: Vec<Stmt>,    //[Return(Some(Call(int, [Ident(a)])))]
    pub span: Span,
}

//in a function or system:
//(arg: string)
#[derive(Debug)]
pub struct Param {
    pub name: String, //arg
    pub ty: Type,     //string
}

//[q : ent(Foo, Bar)]
//sys scale_foos_by_bars(scale: float){
//  q.foo.value *= q.bar.value;
//}
#[derive(Debug)]
pub struct SysDecl {
    pub query: Query,       //[(q, [Foo, Bar])]
    pub name: String,       //scale_foos_by_bars
    pub params: Vec<Param>, //[(scale, float)]
    pub body: Vec<Stmt>,    //[Binary(FieldAccess(...), MulAssn, FieldAccess(...))]
    pub span: Span,
}

//[q: ent(Foo, Bar, Baz)]
#[derive(Debug)]
pub struct Query {
    pub bindings: Vec<(String, Vec<String>)>, // [(q, [Foo, Bar, Baz])]
}

//let foo = 5;
//foo = 6;
//return true;
//9 + 6
//if true { ... } else { ... }
#[derive(Debug)]
pub enum Stmt {
    Let {
        name: String,
        value: Expr,
    }, //foo, 5
    Assign {
        target: LValue,
        value: Expr,
    }, //foo, 6
    Return(Option<Expr>), //Some(true)
    If {
        condition: Expr,              //true
        then_body: Vec<Stmt>,         //...
        else_body: Option<Vec<Stmt>>, //...
    },
    Expr(Expr), //Binary(9, Add, 6)
}

//guy
//guy.pos
#[derive(Debug)]
pub enum LValue {
    Ident(String),                                      //guy
    FieldAccess { object: Box<LValue>, field: String }, //guy, pos
}

#[derive(Debug)]
pub enum Expr {
    Ident(String),         //x
    Int(i64),              //10
    Float(f64),            //0.5
    StringLiteral(String), //"hi"
    Bool(bool),            //true
    FieldAccess {
        //x.y
        object: Box<Expr>, //x
        field: String,     //y
    },
    Call {
        //f(x, y)
        callee: String,  //f
        args: Vec<Expr>, //[x, y]
    },
    Binary {
        // 5 > 3
        left: Box<Expr>,  //5
        op: BinOp,        //GT
        right: Box<Expr>, //3
    },
    Unary {
        // !false
        op: UnOp,        //Not
        expr: Box<Expr>, //false
    },
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    And,
    Or,
}

#[derive(Debug, Clone, Copy)]
pub enum UnOp {
    Neg,
    Not,
}

//operator precedences
fn precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 1, //weakest priority
        BinOp::And => 2,
        BinOp::Eq | BinOp::Neq => 3,
        BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => 4,
        BinOp::Add | BinOp::Sub => 5,
        BinOp::Mul | BinOp::Div => 6, //strongest priority
    }
}

fn is_binary_op(token: &Token) -> Option<BinOp> {
    match token {
        Token::Plus => Some(BinOp::Add),
        Token::Minus => Some(BinOp::Sub),
        Token::Star => Some(BinOp::Mul),
        Token::Slash => Some(BinOp::Div),
        Token::Eq => Some(BinOp::Eq),
        Token::Neq => Some(BinOp::Neq),
        Token::Lt => Some(BinOp::Lt),
        Token::Gt => Some(BinOp::Gt),
        Token::Lte => Some(BinOp::Lte),
        Token::Gte => Some(BinOp::Gte),
        Token::And => Some(BinOp::And),
        Token::Or => Some(BinOp::Or),
        _ => None,
    }
}

pub struct Parser {
    //all tokens
    tokens: Vec<SpannedToken>,

    //cursor into tomens
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn can_start_expr(&self) -> bool {
        matches!(
            self.peek(),
            Token::Ident(_)
                | Token::Int(_)
                | Token::Float(_)
                | Token::StringLiteral(_)
                | Token::True
                | Token::False
                | Token::LParen
                | Token::Minus
                | Token::Not
        )
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].token.clone();
        self.pos += 1;
        tok
    }

    fn expect(&mut self, expected: &Token) -> ParseResult<Span> {
        if self.peek() == expected {
            let span = self.peek_span();
            self.advance();
            Ok(span)
        } else {
            Err(ParseError {
                message: format!("Expected {:?}, found {:?}", expected, self.peek()),
                span: self.peek_span(),
            })
        }
    }

    fn expect_ident(&mut self) -> ParseResult<(String, Span)> {
        match self.peek() {
            Token::Ident(s) => {
                let span = self.peek_span();
                let s = s.clone();
                self.advance();
                Ok((s, span))
            }
            _ => Err(ParseError {
                message: format!("Expected identifier, found {:?}", self.peek()),
                span: self.peek_span(),
            }),
        }
    }

    fn at_end(&self) -> bool {
        matches!(self.peek(), Token::EOF)
    }

    fn parse_ident_list(&mut self, end: Token) -> ParseResult<Vec<String>> {
        let mut items = vec![];
        while self.peek() != &end {
            if !matches!(self.peek(), Token::Ident(_)) {
                break;
            }

            let (ident, _) = self.expect_ident()?;
            items.push(ident);

            if self.peek() == &Token::Comma {
                self.advance();
            } else if self.peek() != &end {
                return Err(ParseError {
                    message: format!("Expected ',' or '{:?}' in list", end),
                    span: self.peek_span(),
                });
            }
        }
        self.expect(&end)?;
        Ok(items)
    }

    fn parse_params(&mut self) -> ParseResult<Vec<Param>> {
        self.expect(&Token::LParen)?;
        let mut params = vec![];
        while self.peek() != &Token::RParen {
            if !matches!(self.peek(), Token::Ident(_)) {
                break;
            }

            let (name, _) = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let ty = self.parse_type()?;
            params.push(Param { name, ty });

            if self.peek() == &Token::Comma {
                self.advance();
            } else if self.peek() != &Token::RParen {
                return Err(ParseError {
                    message: "Expected ',' or ')' after parameter".to_string(),
                    span: self.peek_span(),
                });
            }
        }
        self.expect(&Token::RParen)?;
        Ok(params)
    }

    fn parse_type(&mut self) -> ParseResult<Type> {
        match self.advance() {
            Token::IntType => Ok(Type::Int),
            Token::StringType => Ok(Type::String),
            Token::FloatType => Ok(Type::Float),
            Token::VoidType => Ok(Type::Void),
            Token::BoolType => Ok(Type::Bool),
            Token::Ident(s) => Ok(Type::Custom(s)),
            tok => Err(ParseError {
                message: format!("Expected type, found {:?}", tok),
                span: self.peek_span(),
            }),
        }
    }

    fn parse_return_type(&mut self) -> ParseResult<Type> {
        if self.peek() == &Token::RParen {
            Ok(Type::Void)
        } else {
            self.parse_type()
        }
    }

    fn parse_block(&mut self) -> ParseResult<Vec<Stmt>> {
        self.expect(&Token::LBrace)?;
        let mut stmts = vec![];
        while self.peek() != &Token::RBrace {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(stmts)
    }

    pub fn parse(mut self) -> ParseResult<Program> {
        let mut decls = vec![];

        while !self.at_end() {
            let decl = match self.peek() {
                Token::Comp => self.parse_comp_decl().map(Decl::Comp)?,
                Token::Ent => self.parse_ent_decl().map(Decl::Ent)?,
                Token::Fun => self.parse_fun_decl().map(Decl::Fun)?,
                Token::Let => self.parse_let_decl().map(Decl::Let)?,
                Token::LBracket => self.parse_sys_decl().map(Decl::Sys)?,
                tok => {
                    return Err(ParseError {
                        message: format!("Unexpected token: {:?}", tok),
                        span: self.peek_span(),
                    })
                }
            };
            decls.push(decl);
        }

        Ok(Program { decls })
    }

    fn parse_comp_decl(&mut self) -> ParseResult<CompDecl> {
        let start = self.peek_span();
        self.expect(&Token::Comp)?;
        let (name, _) = self.expect_ident()?;

        self.expect(&Token::LParen)?;
        let mut fields = vec![];
        while self.peek() != &Token::RParen {
            if !matches!(self.peek(), Token::Ident(_)) {
                break;
            }

            let (field_name, _) = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let ty = self.parse_type()?;
            fields.push(Field {
                name: field_name,
                ty,
            });

            if self.peek() == &Token::Comma {
                self.advance();
            } else if self.peek() != &Token::RParen {
                return Err(ParseError {
                    message: "Expected ',' or ')' after component field".to_string(),
                    span: self.peek_span(),
                });
            }
        }

        self.expect(&Token::RParen)?;
        self.expect(&Token::Semicolon)?;

        Ok(CompDecl {
            name,
            fields,
            span: Span::new(start.start, self.tokens[self.pos - 1].span.end, 0, 0),
        })
    }

    fn parse_ent_decl(&mut self) -> ParseResult<EntDecl> {
        let start = self.peek_span();
        self.expect(&Token::Ent)?;
        let (name, _) = self.expect_ident()?;

        self.expect(&Token::LParen)?;
        let comps = self.parse_ident_list(Token::RParen)?;

        self.expect(&Token::Semicolon)?;

        Ok(EntDecl {
            name,
            comps,
            span: Span::new(start.start, self.tokens[self.pos - 1].span.end, 0, 0),
        })
    }

    fn parse_fun_decl(&mut self) -> ParseResult<FunDecl> {
        let start = self.peek_span();
        self.expect(&Token::Fun)?;

        self.expect(&Token::LParen)?;
        let ret = self.parse_return_type()?;
        self.expect(&Token::RParen)?;

        let (name, _) = self.expect_ident()?;
        let params = self.parse_params()?;
        let body = self.parse_block()?;

        Ok(FunDecl {
            name,
            params,
            ret,
            body,
            span: Span::new(start.start, self.tokens[self.pos - 1].span.end, 0, 0),
        })
    }

    fn parse_sys_decl(&mut self) -> ParseResult<SysDecl> {
        let start = self.peek_span();
        self.expect(&Token::LBracket)?;

        //parse bindings: [q: EntName] or [q: ent(Comp1, Comp2)]
        let mut bindings = vec![];
        loop {
            let (var, _) = self.expect_ident()?;
            self.expect(&Token::Colon)?;

            if self.peek() == &Token::Ent {
                self.advance();
                self.expect(&Token::LParen)?;
                let comps = self.parse_ident_list(Token::RParen)?;
                self.expect(&Token::RParen)?;
                bindings.push((var, comps));
            } else {
                let (comp, _) = self.expect_ident()?;
                bindings.push((var, vec![comp]));
            }

            if self.peek() == &Token::Comma {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(&Token::RBracket)?;
        self.expect(&Token::Sys)?;

 

        let (name, _) = self.expect_ident()?;
        let params = self.parse_params()?;
        let body = self.parse_block()?;

        Ok(SysDecl {
            query: Query { bindings },
            name,
            params,
            span: Span::new(start.start, self.tokens[self.pos - 1].span.end, 0, 0),
            body,
        })
    }

    fn parse_let_decl(&mut self) -> ParseResult<LetDecl> {
        let start = self.peek_span();
        self.expect(&Token::Let)?;
        let (name, _) = self.expect_ident()?;
        self.expect(&Token::Assign)?;
        let value = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;

        Ok(LetDecl {
            name,
            value,
            span: Span::new(start.start, self.tokens[self.pos - 1].span.end, 0, 0),
        })
    }

    fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        match self.peek() {
            Token::Let => {
                self.advance();
                let (name, _) = self.expect_ident()?;
                self.expect(&Token::Assign)?;
                let value = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                Ok(Stmt::Let { name, value })
            }
            Token::Return => {
                self.advance();
                if self.peek() == &Token::Semicolon {
                    self.advance();
                    Ok(Stmt::Return(None))
                } else {
                    let expr = self.parse_expr()?;
                    self.expect(&Token::Semicolon)?;
                    Ok(Stmt::Return(Some(expr)))
                }
            }
            Token::If => {
                self.advance();
                let condition = self.parse_expr()?;
                let then_body = self.parse_block()?;

                let else_body = if self.peek() == &Token::Else {
                    self.advance();
                    Some(self.parse_block()?)
                } else {
                    None
                };

                Ok(Stmt::If {
                    condition,
                    then_body,
                    else_body,
                })
            }
            _ => {
                let expr = self.parse_expr()?;

                if self.peek() == &Token::Assign {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.expect(&Token::Semicolon)?;

                    //get assignable lvalue from arbitrary expr
                    //(in practice Ident or FieldAccess)
                    let target = expr_to_lvalue(expr)?;
                    Ok(Stmt::Assign { target, value })
                } else {
                    self.expect(&Token::Semicolon)?;
                    Ok(Stmt::Expr(expr))
                }
            }
        }
    }

    fn parse_expr(&mut self) -> ParseResult<Expr> {
        self.parse_precedence(0)
    }

    fn parse_precedence(&mut self, min_prec: u8) -> ParseResult<Expr> {
        let mut left = self.parse_unary()?;

        while let Some(op) = is_binary_op(self.peek()) {
            let prec = precedence(op);
            if prec < min_prec {
                break;
            }
            self.advance();
            let right = self.parse_precedence(prec + 1)?; //left associative
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> ParseResult<Expr> {
        match self.peek() {
            Token::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnOp::Neg,
                    expr: Box::new(expr),
                })
            }
            Token::Not => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnOp::Not,
                    expr: Box::new(expr),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> ParseResult<Expr> {
        let mut expr = self.parse_primary()?;

        while self.peek() == &Token::Dot {
            self.advance();
            let (field, _) = self.expect_ident()?;
            expr = Expr::FieldAccess {
                object: Box::new(expr),
                field,
            };
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> ParseResult<Expr> {
        match self.peek().clone() {
            Token::Ident(s) => {
                let s = s.clone();
                self.advance();

                if self.peek() == &Token::LParen {
                    self.advance();
                    let mut args = vec![];
                    while self.peek() != &Token::RParen {
                        if !self.can_start_expr() {
                            break;
                        }
                        args.push(self.parse_expr()?);
                        if self.peek() == &Token::Comma {
                            self.advance();
                        }
                    }
                    self.expect(&Token::RParen)?;
                    Ok(Expr::Call { callee: s, args })
                } else {
                    Ok(Expr::Ident(s))
                }
            }
            Token::Int(n) => {
                let n = n;
                self.advance();
                Ok(Expr::Int(n))
            }
            Token::Float(n) => {
                let n = n;
                self.advance();
                Ok(Expr::Float(n))
            }
            Token::StringLiteral(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::StringLiteral(s))
            }
            Token::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            tok => Err(ParseError {
                message: format!("Expected expression, found {:?}", tok),
                span: self.peek_span(),
            }),
        }
    }
}

fn expr_to_lvalue(expr: Expr) -> ParseResult<LValue> {
    match expr {
        Expr::Ident(name) => Ok(LValue::Ident(name)),
        Expr::FieldAccess { object, field } => {
            let object = expr_to_lvalue(*object)?;
            Ok(LValue::FieldAccess {
                object: Box::new(object),
                field,
            })
        }
        _ => Err(ParseError {
            message: "Invalid assignment target".to_string(),
            span: Span::new(0, 0, 0, 0), //TODO: track spans in Expr
        }),
    }
}

pub fn parse(tokens: Vec<SpannedToken>) -> ParseResult<Program> {
    Parser::new(tokens).parse()
}
