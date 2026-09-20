pub mod language;
pub mod backend;
pub mod syntax;
pub mod c99;

use std::{collections::HashMap, env, fs};
use backend::Backend;
use c99::C99Backend;
use language::{AstFlow, Derivative, Effect, Signature, Space, TypeChecker, TypedFlow};
use syntax::parse;

fn builtins() -> HashMap<String, Signature> {
    let mut p = HashMap::new();
    let pure = |domain, codomain, derivative| Signature { domain, codomain, effects: vec![Effect::Pure], derivative };
    p.insert("add".into(), pure(Space::Scalar("f64".into()), Space::Scalar("f64".into()), Derivative::Analytic));
    p.insert("mul".into(), pure(Space::Scalar("f64".into()), Space::Scalar("f64".into()), Derivative::Analytic));
    p.insert("neural_layer".into(), pure(Space::Tensor { element: "f32".into(), shape: vec![128] }, Space::Tensor { element: "f32".into(), shape: vec![64] }, Derivative::ReverseMode));
    p.insert("quantum_gate".into(), Signature { domain: Space::Quantum { qubits: 2 }, codomain: Space::Quantum { qubits: 2 }, effects: vec![Effect::DeviceIo], derivative: Derivative::ReverseMode });
    p.insert("organoid_step".into(), Signature { domain: Space::Organoid { pins: 64 }, codomain: Space::Organoid { pins: 64 }, effects: vec![Effect::DeviceIo], derivative: Derivative::ReverseMode });
    p.insert("organoid_pulse".into(), Signature { domain: Space::Organoid { pins: 64 }, codomain: Space::Organoid { pins: 64 }, effects: vec![Effect::DeviceIo], derivative: Derivative::ReverseMode });
    p.insert("logic_step".into(), pure(Space::Logic { bits: 8 }, Space::Logic { bits: 8 }, Derivative::StraightThrough));
    p
}

fn validate_backend(flow: &TypedFlow) -> Result<(), String> {
    match flow {
        TypedFlow::Primitive { signature, .. } => C99Backend::new().supports_signature(signature),
        TypedFlow::Identity(_) => Ok(()),
        TypedFlow::Chain(a, b) | TypedFlow::Parallel(a, b) => { validate_backend(a)?; validate_backend(b) },
        TypedFlow::Feedback { inner, .. } | TypedFlow::Gradient { inner, .. } => validate_backend(inner),
    }
}

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| "program.cat".into());
    let source = match fs::read_to_string(&path) { Ok(s) => s, Err(e) => { eprintln!("read error: {e}"); std::process::exit(1); } };
    let mut primitives = builtins();
    let program = match parse(&source, &mut primitives) { Ok(p) => p, Err(e) => { eprintln!("parse error: {e}"); std::process::exit(1); } };
    let mut typed_flows = HashMap::new();
    for declaration in &program.flows {
        let checker = TypeChecker { primitives: &primitives, flows: &typed_flows };
        let typed = match checker.check(&declaration.body) { Ok(f) => f, Err(e) => { eprintln!("type error in {}: {e}", declaration.name); std::process::exit(1); } };
        let inferred = typed.signature();
        if let Some((domain, codomain)) = &declaration.signature { if inferred != (domain.clone(), codomain.clone()) { eprintln!("signature mismatch in {}: inferred {:?}, declared {:?}", declaration.name, inferred, (domain, codomain)); std::process::exit(1); } }
        if let Err(e) = validate_backend(&typed) { eprintln!("backend error in {}: {e}", declaration.name); std::process::exit(1); }
        typed_flows.insert(declaration.name.clone(), typed);
    }
    let backend = C99Backend::new();
    let mut output = backend.preamble();
    for declaration in &program.flows {
        if let Some(flow) = typed_flows.get(&declaration.name) { output.push_str(&backend.lower(flow).unwrap_or_else(|e| format!("/* lowering error: {e} */\n"))); }
    }
    output.push_str("int main(void) { return 0; }\n");
    if let Err(e) = fs::write("payload.c", output) { eprintln!("write error: {e}"); std::process::exit(1); }
    println!("Generated payload.c");
}

#[cfg(test)]
mod tests {
    use super::*;
    fn checker<'a>(p: &'a HashMap<String, Signature>, f: &'a HashMap<String, TypedFlow>) -> TypeChecker<'a> { TypeChecker { primitives: p, flows: f } }
    #[test] fn composition_types() { let mut p = builtins(); let program = parse("flow x = neural_layer\n", &mut p).unwrap(); let t = checker(&p, &HashMap::new()).check(&program.flows[0].body).unwrap(); assert_eq!(t.signature().0, Space::Tensor { element: "f32".into(), shape: vec![128] }); }
    #[test] fn products_preserve_order() { let mut p = builtins(); let program = parse("flow x = neural_layer || quantum_gate\n", &mut p).unwrap(); let t = checker(&p, &HashMap::new()).check(&program.flows[0].body).unwrap(); assert_eq!(t.signature().0, Space::Product(vec![Space::Tensor { element: "f32".into(), shape: vec![128] }, Space::Quantum { qubits: 2 }])); }
    #[test] fn feedback_requires_matching_trace() { let mut p = builtins(); let program = parse("flow x = ~(neural_layer || quantum_gate || organoid_pulse)\n", &mut p).unwrap(); let t = checker(&p, &HashMap::new()).check(&program.flows[0].body).unwrap(); assert_eq!(t.signature().0, Space::Product(vec![Space::Tensor { element: "f32".into(), shape: vec![128] }, Space::Quantum { qubits: 2 }])); }
    #[test] fn gradient_reverses_signature() { let mut p = builtins(); let program = parse("flow x = grad(neural_layer)\n", &mut p).unwrap(); let t = checker(&p, &HashMap::new()).check(&program.flows[0].body).unwrap(); assert_eq!(t.signature().1, Space::Tensor { element: "f32".into(), shape: vec![128] }); }
}
