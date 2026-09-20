use crate::backend::Backend;
use crate::c99::C99Backend;
use crate::language::default_primitives;
use std::fs;

fn main() {
    let source = fs::read_to_string("program.cat").expect("read program.cat");
    let primitives = default_primitives();
    let program = crate::syntax::parse_program(&source, &primitives).expect("parse program");
    let backend = C99Backend::new();
    let out = backend.lower_program(&program).expect("lower to C99");
    fs::write("payload.c", out).expect("write payload.c");
    println!("Generated payload.c");
}
