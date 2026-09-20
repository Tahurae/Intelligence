use crate::language::{Flow, Program, Signature, Space, typecheck};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    PortableC99,
}

pub trait Backend {
    fn kind(&self) -> BackendKind;
    fn supports_space(&self, space: &Space) -> bool;
    fn lower(&self, flow: &Flow) -> Result<String, String>;
    fn lower_program(&self, program: &Program) -> Result<String, String>;
}

pub struct C99Backend;

impl C99Backend {
    pub fn new() -> Self {
        Self
    }
}

impl Backend for C99Backend {
    fn kind(&self) -> BackendKind { BackendKind::PortableC99 }

    fn supports_space(&self, space: &Space) -> bool {
        matches!(
            space,
            Space::Unit
                | Space::Scalar(_)
                | Space::Tensor { .. }
                | Space::Quantum { .. }
                | Space::Organoid { .. }
                | Space::Logic { .. }
                | Space::Product(_)
        )
    }

    fn lower(&self, flow: &Flow) -> Result<String, String> {
        if !self.supports_space(&flow.domain()) || !self.supports_space(&flow.codomain()) {
            return Err("unsupported space for C99 backend".to_string());
        }
        Ok(crate::language::lower_flow(flow))
    }

    fn lower_program(&self, program: &Program) -> Result<String, String> {
        let mut out = String::from("#include <stdint.h>\n#include <stddef.h>\n\n");
        for (name, flow) in &program.flows {
            let mut flow_code = self.lower(flow)?;
            out.push_str(&format!("/* flow {name} */\n{flow_code}\n"));
        }
        out.push_str("int main(void) { return 0; }\n");
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::{default_primitives, Flow, Space};

    #[test]
    fn backend_supports_typed_spaces() {
        let backend = C99Backend::new();
        assert!(backend.supports_space(&Space::Scalar("f64".into())));
        assert!(backend.supports_space(&Space::Tensor { element: "f32".into(), shape: vec![128] }));
        assert!(backend.supports_space(&Space::Product(vec![Space::Scalar("f64".into()), Space::Scalar("f64".into())])));
    }

    #[test]
    fn lower_program_emits_main() {
        let mut flows = HashMap::new();
        flows.insert(
            "alpha".to_string(),
            Flow::Primitive {
                name: "add".to_string(),
                domain: Space::Scalar("f64".into()),
                codomain: Space::Scalar("f64".into()),
            },
        );
        let program = Program { spaces: HashMap::new(), flows };
        let backend = C99Backend::new();
        let generated = backend.lower_program(&program).unwrap();
        assert!(generated.contains("int main(void)"));
    }
}










































































































