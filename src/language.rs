use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Space {
    Unit,
    Scalar(String),
    Tensor { element: String, shape: Vec<usize> },
    Quantum { qubits: usize },
    Organoid { pins: usize },
    Logic { bits: usize },
    Product(Vec<Space>),
}

impl Space {
    pub fn product(left: Self, right: Self) -> Self {
        let mut parts = match left {
            Self::Product(mut rest) => {
                let mut acc = Vec::new();
                acc.append(&mut rest);
                acc
            }
            other => vec![other],
        };

        match right {
            Self::Product(mut rest) => parts.append(&mut rest),
            other => parts.push(other),
        }

        Self::Product(parts)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Pure,
    DeviceIo,
    HostIo,
    Nondeterministic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Derivative {
    Analytic,
    ReverseMode,
    StraightThrough,
    NonDifferentiable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub domain: Space,
    pub codomain: Space,
    pub derivative: Derivative,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flow {
    Primitive { name: String, domain: Space, codomain: Space },
    Identity(Space),
    Chain(Box<Flow>, Box<Flow>),
    Parallel(Box<Flow>, Box<Flow>),
    Feedback(Box<Flow>),
    Gradient(Box<Flow>),
}

impl Flow {
    pub fn domain(&self) -> Space {
        match self {
            Self::Primitive { domain, .. } => domain.clone(),
            Self::Identity(space) => space.clone(),
            Self::Chain(left, _) => left.domain(),
            Self::Parallel(left, right) => Space::Product(vec![left.domain(), right.domain()]),
            Self::Feedback(inner) => inner.domain(),
            Self::Gradient(inner) => inner.codomain(),
        }
    }

    pub fn codomain(&self) -> Space {
        match self {
            Self::Primitive { codomain, .. } => codomain.clone(),
            Self::Identity(space) => space.clone(),
            Self::Chain(_, right) => right.codomain(),
            Self::Parallel(left, right) => Space::Product(vec![left.codomain(), right.codomain()]),
            Self::Feedback(inner) => inner.codomain(),
            Self::Gradient(inner) => inner.domain(),
        }
    }
}

pub fn default_primitives() -> HashMap<String, Signature> {
    let mut map = HashMap::new();
    let scalar = |dom: Space, cod: Space, der: Derivative| Signature {
        domain: dom,
        codomain: cod,
        derivative: der,
        effects: vec![Effect::Pure],
    };

    map.insert(
        "add".to_string(),
        scalar(Space::Scalar("f64".into()), Space::Scalar("f64".into()), Derivative::Analytic),
    );
    map.insert(
        "mul".to_string(),
        scalar(Space::Scalar("f64".into()), Space::Scalar("f64".into()), Derivative::Analytic),
    );
    map.insert(
        "neural_layer".to_string(),
        Signature {
            domain: Space::Tensor { element: "f32".into(), shape: vec![128] },
            codomain: Space::Tensor { element: "f32".into(), shape: vec![64] },
            derivative: Derivative::ReverseMode,
            effects: vec![Effect::Pure],
        },
    );
    map.insert(
        "quantum_gate".to_string(),
        Signature {
            domain: Space::Quantum { qubits: 2 },
            codomain: Space::Quantum { qubits: 2 },
            derivative: Derivative::ReverseMode,
            effects: vec![Effect::DeviceIo],
        },
    );
    map.insert(
        "organoid_pulse".to_string(),
        Signature {
            domain: Space::Organoid { pins: 64 },
            codomain: Space::Organoid { pins: 64 },
            derivative: Derivative::ReverseMode,
            effects: vec![Effect::DeviceIo],
        },
    );
    map.insert(
        "logic_step".to_string(),
        Signature {
            domain: Space::Logic { bits: 8 },
            codomain: Space::Logic { bits: 8 },
            derivative: Derivative::StraightThrough,
            effects: vec![Effect::Pure],
        },
    );

    map
}

pub fn typecheck(flow: &Flow, registry: &HashMap<String, Signature>) -> Result<(), String> {
    match flow {
        Flow::Primitive { name, domain, codomain } => {
            let signature = registry.get(name).ok_or_else(|| format!("unknown primitive `{name}`"))?;
            if signature.domain != *domain || signature.codomain != *codomain {
                return Err(format!(
                    "primitive `{name}` requires {:?} -> {:?}, got {:?} -> {:?}",
                    signature.domain, signature.codomain, domain, codomain
                ));
            }
            Ok(())
        }
        Flow::Identity(_) => Ok(()),
        Flow::Chain(left, right) => {
            typecheck(left, registry)?;
            typecheck(right, registry)?;
            if left.codomain() != right.domain() {
                return Err(format!(
                    "composition mismatch: {:?} -> {:?} cannot feed {:?} -> {:?}",
                    left.domain(), left.codomain(), right.domain(), right.codomain()
                ));
            }
            Ok(())
        }
        Flow::Parallel(left, right) => {
            typecheck(left, registry)?;
            typecheck(right, registry)?;
            Ok(())
        }
        Flow::Feedback(inner) => {
            typecheck(inner, registry)?;
            let domain = inner.domain();
            let codomain = inner.codomain();
            if domain != codomain {
                return Err(format!("feedback requires matching trace: {:?} != {:?}", domain, codomain));
            }
            Ok(())
        }
        Flow::Gradient(inner) => {
            typecheck(inner, registry)?;
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub spaces: HashMap<String, Space>,
    pub flows: HashMap<String, Flow>,
}

pub fn lower_flow(flow: &Flow) -> String {
    match flow {
        Flow::Primitive { name, .. } => format!("/* {name} */\n"),
        Flow::Identity(space) => format!("/* identity {:?} */\n", space),
        Flow::Chain(left, right) => {
            let mut out = String::new();
            out.push_str(&lower_flow(left));
            out.push_str(&lower_flow(right));
            out
        }
        Flow::Parallel(left, right) => {
            let mut out = String::new();
            out.push_str(&lower_flow(left));
            out.push_str(&lower_flow(right));
            out
        }
        Flow::Feedback(inner) => {
            let mut out = String::new();
            out.push_str("/* feedback trace */\n");
            out.push_str(&lower_flow(inner));
            out
        }
        Flow::Gradient(inner) => {
            let mut out = String::new();
            out.push_str("/* grad */\n");
            out.push_str(&lower_flow(inner));
            out
        }
    }
}
























































































