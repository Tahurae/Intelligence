//! Backend-independent language model for Intelligence.
//!
//! This module deliberately contains no OS, device, CUDA, OpenQASM, or C99
//! assumptions. Backends consume the typed IR defined here.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    pub fn product(left: Space, right: Space) -> Self {
        let mut parts = match left { Self::Product(parts) => parts, value => vec![value] };
        match right { Self::Product(other) => parts.extend(other), value => parts.push(value) }
        Self::Product(parts)
    }

    pub fn parts(&self) -> Vec<Space> {
        match self { Self::Product(parts) => parts.clone(), value => vec![value.clone()] }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect { Pure, Allocation, HostIo, DeviceIo, Nondeterministic }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Derivative { Analytic, ReverseMode, StraightThrough, NonDifferentiable }

#[derive(Debug, Clone)]
pub struct Signature {
    pub domain: Space,
    pub codomain: Space,
    pub effects: Vec<Effect>,
    pub derivative: Derivative,
}

#[derive(Debug, Clone)]
pub enum AstFlow {
    Name(String),
    Identity(Space),
    Chain(Box<AstFlow>, Box<AstFlow>),
    Parallel(Box<AstFlow>, Box<AstFlow>),
    Feedback(Box<AstFlow>),
    Gradient(Box<AstFlow>),
}

#[derive(Debug, Clone)]
pub struct FlowDecl { pub name: String, pub signature: Option<(Space, Space)>, pub body: AstFlow }

#[derive(Debug, Clone)]
pub enum TypedFlow {
    Primitive { name: String, signature: Signature },
    Identity(Space),
    Chain(Box<TypedFlow>, Box<TypedFlow>),
    Parallel(Box<TypedFlow>, Box<TypedFlow>),
    Feedback { inner: Box<TypedFlow>, trace: Space, domain: Space, codomain: Space },
    Gradient { inner: Box<TypedFlow>, domain: Space, codomain: Space },
}

impl TypedFlow {
    pub fn signature(&self) -> (Space, Space) {
        match self {
            Self::Primitive { signature, .. } => (signature.domain.clone(), signature.codomain.clone()),
            Self::Identity(space) => (space.clone(), space.clone()),
            Self::Chain(left, right) => (left.signature().0, right.signature().1),
            Self::Parallel(left, right) => (Space::product(left.signature().0, right.signature().0), Space::product(left.signature().1, right.signature().1)),
            Self::Feedback { domain, codomain, .. } | Self::Gradient { domain, codomain, .. } => (domain.clone(), codomain.clone()),
        }
    }
}

pub struct TypeChecker<'a> {
    pub primitives: &'a HashMap<String, Signature>,
    pub flows: &'a HashMap<String, TypedFlow>,
}

impl<'a> TypeChecker<'a> {
    pub fn check(&self, flow: &AstFlow) -> Result<TypedFlow, String> {
        match flow {
            AstFlow::Name(name) => {
                if let Some(signature) = self.primitives.get(name) { return Ok(TypedFlow::Primitive { name: name.clone(), signature: signature.clone() }); }
                self.flows.get(name).cloned().ok_or_else(|| format!("undefined name `{name}`"))
            }
            AstFlow::Identity(space) => Ok(TypedFlow::Identity(space.clone())),
            AstFlow::Chain(left, right) => {
                let left = self.check(left)?; let right = self.check(right)?;
                let left_out = left.signature().1; let right_in = right.signature().0;
                if left_out != right_in { return Err(format!("composition mismatch: {:?} cannot feed {:?}", left_out, right_in)); }
                Ok(TypedFlow::Chain(Box::new(left), Box::new(right)))
            }
            AstFlow::Parallel(left, right) => Ok(TypedFlow::Parallel(Box::new(self.check(left)?), Box::new(self.check(right)?))),
            AstFlow::Feedback(inner) => {
                let inner = self.check(inner)?; let (domain, codomain) = inner.signature();
                let input = domain.parts(); let output = codomain.parts();
                if input.len() < 2 || output.len() < 2 { return Err("feedback requires f : A || U -> B || U".into()); }
                let input_trace = input.last().unwrap(); let output_trace = output.last().unwrap();
                if input_trace != output_trace { return Err(format!("feedback trace mismatch: {:?} != {:?}", input_trace, output_trace)); }
                let domain = collapse(Space::Product(input[..input.len()-1].to_vec()));
                let codomain = collapse(Space::Product(output[..output.len()-1].to_vec()));
                Ok(TypedFlow::Feedback { inner: Box::new(inner), trace: input_trace.clone(), domain, codomain })
            }
            AstFlow::Gradient(inner) => {
                let inner = self.check(inner)?; let (domain, codomain) = inner.signature();
                if matches!(inner, TypedFlow::Primitive { signature: Signature { derivative: Derivative::NonDifferentiable, .. }, .. }) { return Err("cannot differentiate a non-differentiable primitive".into()); }
                Ok(TypedFlow::Gradient { inner: Box::new(inner), domain: Space::product(domain, codomain.clone()), codomain: domain })
            }
        }
    }
}

fn collapse(space: Space) -> Space { match space { Space::Product(mut parts) if parts.len() == 1 => parts.remove(0), value => value } }
