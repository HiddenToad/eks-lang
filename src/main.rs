use std::env;
use std::fs;
use std::path::Path;

mod codegen;
mod lex;
mod parse;
mod utils;

pub use codegen::*;
use inkwell::context::Context;
use inkwell::targets::CodeModel;
use inkwell::targets::FileType;
use inkwell::targets::InitializationConfig;
use inkwell::targets::RelocMode;
use inkwell::targets::Target;
use inkwell::targets::TargetMachine;
pub use lex::*;
pub use parse::*;

fn main() {
    let filename = env::args().nth(1).unwrap();
    let contents = fs::read_to_string(filename).unwrap();

    let tokens = lex(contents);
    println!("{:?}", tokens);

    let program = parse(tokens).unwrap();
    println!("{:#?}", program);

    let context = Context::create();
    let mut codegen = Codegen::new(&context);

    match codegen.compile(&program) {
        Ok(_) => {
            println!("Compilation successful!\n");
            println!("=== LLVM IR ===");
            println!("{}", codegen.print_ir());
        }
        Err(e) => {
            eprintln!("Codegen error: {}", e);
            std::process::exit(1);
        }
    }

    Target::initialize_native(&InitializationConfig::default())
        .expect("Failed to initialize native target");

    let target_triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&target_triple).expect("Failed to get target");

    let target_machine = target
        .create_target_machine(
            &target_triple,
            "generic",
            "",
            inkwell::OptimizationLevel::Default, //IR is already optimized, keep default here
            RelocMode::PIC,
            CodeModel::Default,
        )
        .expect("Failed to create target machine");

    println!("Generating target.o...");
    target_machine
        .write_to_file(&codegen.module, FileType::Object, Path::new("target.o"))
        .expect("Failed to write object file");

    println!("Linking executable...");
    let link_status = std::process::Command::new("cc")
        .args([
            "target.o",
            "-o",
            env::args().nth(1).unwrap().split(".").next().unwrap(),
        ])
        .status()
        .expect("Failed to run linker (Do you have gcc or clang installed?)");

    if link_status.success() {
        println!(
            "Success! Run it with ./{}",
            env::args().nth(1).unwrap().split(".").next().unwrap()
        );
    } else {
        eprintln!("Linking failed.");
    }
}
