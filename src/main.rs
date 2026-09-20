use std::collections::HashMap;
use std::fs;
use std::path::Path;

mod backend;
mod c99;
mod language;
mod syntax;

use backend::Backend;
use c99::C99Backend;
use language::{default_primitives, Flow, Signature, Space, typecheck};
use syntax::Program;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "program.cat".to_string());
    let source = fs::read_to_string(&path).unwrap_or_else(|err| {
        eprintln!("failed to read `{path}`: {err}");
        std::process::exit(1);
    });

    let primitives = default_primitives();
    let program = syntax::parse_program(&source, &primitives).unwrap_or_else(|err| {
        eprintln!("parse error: {err}");
        std::process::exit(1);
    });

    for flow in program.flows.values() {
        if let Err(err) = typecheck(flow, &primitives) {
            eprintln!("type error: {err}");
            std::process::exit(1);
        }
    }

    let backend = C99Backend::new();
    let c_output = backend.lower_program(&program).unwrap_or_else(|err| {
        eprintln!("backend error: {err}");
        std::process::exit(1);
    });

    let output_path = Path::new("payload.c");
    if let Err(err) = fs::write(output_path, &c_output) {
        eprintln!("failed to write payload.c: {err}");
        std::process::exit(1);
    }

    println!("Generated {}", output_path.display());
}

#[cfg(test)]
mod tests {
    use super::*;
    use language::{Flow, Space};

    #[test]
    fn composition_is_checked() {
        let registry = default_primitives();
        let inner = Flow::Chain(
            Box::new(Flow::Primitive {
                name: "neural_layer".to_string(),
                domain: Space::Tensor { element: "f32".into(), shape: vec![128] },
                codomain: Space::Tensor { element: "f32".into(), shape: vec![64] },
            }),
            Box::new(Flow::Primitive {
                name: "neural_layer".to_string(),
                domain: Space::Tensor { element: "f32".into(), shape: vec![64] },
                codomain: Space::Tensor { element: "f32".into(), shape: vec![16] },
            }),
        );
        assert!(typecheck(&inner, &registry).is_ok());
    }

    #[test]
    fn parallel_product_has_product_space() {
        let registry = default_primitives();
        let flow = Flow::Parallel(
            Box::new(Flow::Primitive {
                name: "neural_layer".to_string(),
                domain: Space::Tensor { element: "f32".into(), shape: vec![128] },
                codomain: Space::Tensor { element: "f32".into(), shape: vec![64] },
            }),
            Box::new(Flow::Primitive {
                name: "quantum_gate".to_string(),
                domain: Space::Quantum { qubits: 2 },
                codomain: Space::Quantum { qubits: 2 },
            }),
        );
        let domain = flow.domain();
        assert_eq!(domain, Space::Product(vec![
            Space::Tensor { element: "f32".into(), shape: vec![128] },
            Space::Quantum { qubits: 2 },
        ]));
        assert!(typecheck(&flow, &registry).is_ok());
    }

    #[test]
    fn feedback_requires_matching_trace() {
        let registry = default_primitives();
        let flow = Flow::Feedback(Box::new(Flow::Parallel(
            Box::new(Flow::Primitive {
                name: "neural_layer".to_string(),
                domain: Space::Tensor { element: "f32".into(), shape: vec![128] },
                codomain: Space::Tensor { element: "f32".into(), shape: vec![64] },
            }),
            Box::new(Flow::Primitive {
                name: "quantum_gate".to_string(),
                domain: Space::Quantum { qubits: 2 },
                codomain: Space::Quantum { qubits: 2 },
            }),
        )));
        assert!(typecheck(&flow, &registry).is_err());
    }

    #[test]
    fn gradients_reverse_domain_and_codomain() {
        let flow = Flow::Gradient(Box::new(Flow::Primitive {
            name: "neural_layer".to_string(),
            domain: Space::Tensor { element: "f32".into(), shape: vec![128] },
            codomain: Space::Tensor { element: "f32".into(), shape: vec![64] },
        }));
        assert_eq!(flow.domain(), Space::Tensor { element: "f32".into(), shape: vec![128] });
        assert_eq!(flow.codomain(), Space::Tensor { element: "f32".into(), shape: vec![64] });
    }
}



















































































































































































































































