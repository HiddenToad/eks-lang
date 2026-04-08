use std::env;
use std::fs;

mod codegen;
mod lex;
mod parse;
mod utils;

pub use codegen::*;
use inkwell::context::Context;
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
}
