use std::env;
use std::fs;

mod parse;
mod lex;
mod utils;

pub use parse::*;
pub use lex::*;

use crate::utils::peek_while;

fn main() {
    let mut s = "abcdefg".chars().peekable();

    let filename = env::args().nth(1).unwrap();
    let contents = fs::read_to_string(filename).unwrap();
    let tokens = lex(contents);
    println!("{:?}", tokens);
    let program = parse(tokens);
    println!("{:#?}", program);
}