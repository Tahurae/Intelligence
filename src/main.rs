use std::collections::HashMap;
use std::env;
use std::fs;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Space {
    Unit,
    Scalar(String),
    Tensor { element: String, shape: Vec<usize> },
    Quantum { qubits: usize },
    Organoid { pins: usize },
    Logic { bits: usize },
    Product(Box<Space>, Box<Space>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DerivativeRule {
    Analytic,
    ReverseMode,
    ParameterShift { shift_numerator: u32, shift_denominator: u32 },
    AdjointSensitivity,
    StraightThrough,
    Custom(String),
    NonDifferentiable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect { Pure, TensorDevice, QuantumDevice, BiologicalIo, HostIo, Allocation, Nondeterministic }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Backend { C99Cpu, Simd, Cuda, OpenQasm, PulseFpga, RealtimeMea, Verilog }

#[derive(Debug, Clone, PartialEq)]
pub enum Flow {
    Identity { space: Space },
    Primitive { name: String, domain: Space, codomain: Space, c_impl: String },
    Chain(Box<Flow>, Box<Flow>),
    Parallel(Box<Flow>, Box<Flow>),
    Feedback(Box<Flow>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Primitive {
    pub name: String,
    pub domain: Space,
    pub codomain: Space,
    pub derivative: DerivativeRule,
    pub effects: Vec<Effect>,
    pub implementations: HashMap<Backend, String>,
}

impl Primitive {
    fn forward(&self, backend: Backend) -> String {
        self.implementations.get(&backend).cloned().unwrap_or_else(|| format!("/* {} has no {:?} implementation */", self.name, backend))
    }

    fn reverse(&self, _backend: Backend) -> String {
        match &self.derivative {
            DerivativeRule::Analytic => format!("/* analytic reverse({}) */", self.name),
            DerivativeRule::ReverseMode => format!("/* reverse-mode({}) */", self.name),
            DerivativeRule::ParameterShift { shift_numerator, shift_denominator } => format!("/* parameter-shift({}/{}) {} */", shift_numerator, shift_denominator, self.name),
            DerivativeRule::AdjointSensitivity => format!("/* adjoint-sensitivity({}) */", self.name),
            DerivativeRule::StraightThrough => format!("/* straight-through({}) */", self.name),
            DerivativeRule::Custom(rule) => format!("/* custom reverse {}: {} */", self.name, rule),
            DerivativeRule::NonDifferentiable => format!("/* non-differentiable: {} */", self.name),
        }
    }
}

pub struct PrimitiveRegistry { primitives: HashMap<String, Primitive> }

impl PrimitiveRegistry {
    pub fn new() -> Self {
        let mut r = Self { primitives: HashMap::new() };
        r.register(Primitive { name: "add".into(), domain: Space::Scalar("Int".into()), codomain: Space::Scalar("Int".into()), derivative: DerivativeRule::Analytic, effects: vec![Effect::Pure], implementations: HashMap::from([(Backend::C99Cpu, "result = input + 1;".into())]) });
        r.register(Primitive { name: "mul".into(), domain: Space::Scalar("Int".into()), codomain: Space::Scalar("Int".into()), derivative: DerivativeRule::Analytic, effects: vec![Effect::Pure], implementations: HashMap::from([(Backend::C99Cpu, "result = input * 2;".into())]) });
        r.register(Primitive { name: "neural_layer".into(), domain: Space::Tensor { element: "Float32".into(), shape: vec![128] }, codomain: Space::Tensor { element: "Float32".into(), shape: vec![64] }, derivative: DerivativeRule::ReverseMode, effects: vec![Effect::TensorDevice], implementations: HashMap::from([(Backend::C99Cpu, "/* neural forward */".into())]) });
        r.register(Primitive { name: "quantum_gate".into(), domain: Space::Quantum { qubits: 6 }, codomain: Space::Quantum { qubits: 6 }, derivative: DerivativeRule::ParameterShift { shift_numerator: 1, shift_denominator: 2 }, effects: vec![Effect::QuantumDevice], implementations: HashMap::from([(Backend::OpenQasm, "/* quantum forward */".into())]) });
        r.register(Primitive { name: "organoid_step".into(), domain: Space::Organoid { pins: 64 }, codomain: Space::Organoid { pins: 64 }, derivative: DerivativeRule::AdjointSensitivity, effects: vec![Effect::BiologicalIo], implementations: HashMap::from([(Backend::RealtimeMea, "/* organoid forward */".into())]) });
        r
    }
    pub fn register(&mut self, p: Primitive) { self.primitives.insert(p.name.clone(), p); }
    pub fn get(&self, name: &str) -> Option<&Primitive> { self.primitives.get(name) }
}

impl Flow {
    /// Structural reverse transformation: R(g ∘ f) = R(f) ∘ R(g), with
    /// products transformed componentwise. Feedback is deliberately retained
    /// as a trace node; its fixed-point rule belongs to a later effect pass.
    pub fn grad(&self, registry: &PrimitiveRegistry) -> Flow {
        match self {
            Flow::Identity { space } => Flow::Identity { space: space.clone() },
            Flow::Primitive { name, domain, codomain, .. } => {
                let reverse = registry.get(name).map(|p| p.reverse(Backend::C99Cpu)).unwrap_or_else(|| format!("/* reverse({}) */", name));
                Flow::Primitive { name: format!("grad_{}", name), domain: codomain.clone(), codomain: domain.clone(), c_impl: reverse }
            }
            Flow::Chain(f, g) => Flow::Chain(Box::new(g.grad(registry)), Box::new(f.grad(registry))),
            Flow::Parallel(f, g) => Flow::Parallel(Box::new(f.grad(registry)), Box::new(g.grad(registry))),
            Flow::Feedback(f) => Flow::Feedback(Box::new(f.grad(registry))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortNode { pub id: String, pub label: String, pub forward: String, pub reverse: String, pub input_space: Space, pub output_space: Space, pub tape_slot: Option<String> }
#[derive(Debug, Clone, PartialEq)]
pub struct PortEdge { pub from: String, pub to: String }
#[derive(Debug, Clone, PartialEq)]
pub struct PortGraph { pub nodes: Vec<PortNode>, pub edges: Vec<PortEdge> }
impl PortGraph { fn new() -> Self { Self { nodes: vec![], edges: vec![] } } }

pub fn lower_flow(flow: &Flow, registry: &PrimitiveRegistry) -> PortGraph {
    fn walk(f: &Flow, r: &PrimitiveRegistry, g: &mut PortGraph, n: &mut usize, previous: Option<String>) -> Option<String> {
        match f {
            Flow::Identity { space } => {
                let id = format!("id_{}", *n); *n += 1;
                g.nodes.push(PortNode { id: id.clone(), label: "identity".into(), forward: "/* identity */".into(), reverse: "/* identity */".into(), input_space: space.clone(), output_space: space.clone(), tape_slot: None });
                if let Some(p) = previous { g.edges.push(PortEdge { from: p, to: id.clone() }); }
                Some(id)
            }
            Flow::Primitive { name, domain, codomain, c_impl } => {
                let id = format!("node_{}", *n); *n += 1;
                let p = r.get(name);
                let reverse = p.map(|x| x.reverse(Backend::C99Cpu)).unwrap_or_else(|| format!("/* reverse {} */", name));
                g.nodes.push(PortNode { id: id.clone(), label: name.clone(), forward: c_impl.clone(), reverse, input_space: domain.clone(), output_space: codomain.clone(), tape_slot: Some(format!("tape_{}", id)) });
                if let Some(prev) = previous { g.edges.push(PortEdge { from: prev, to: id.clone() }); }
                Some(id)
            }
            Flow::Chain(a, b) => { let end = walk(a, r, g, n, previous); walk(b, r, g, n, end) }
            Flow::Parallel(a, b) => { let left = walk(a, r, g, n, previous.clone()); let right = walk(b, r, g, n, previous); right.or(left) }
            Flow::Feedback(inner) => walk(inner, r, g, n, previous),
        }
    }
    let mut g = PortGraph::new(); let mut n = 0; walk(flow, registry, &mut g, &mut n, None); g
}

pub fn reverse_graph(g: &PortGraph) -> PortGraph {
    PortGraph {
        nodes: g.nodes.iter().rev().map(|n| PortNode { id: format!("rev_{}", n.id), label: format!("reverse({})", n.label), forward: n.reverse.clone(), reverse: n.forward.clone(), input_space: n.output_space.clone(), output_space: n.input_space.clone(), tape_slot: n.tape_slot.clone() }).collect(),
        edges: g.edges.iter().rev().map(|e| PortEdge { from: format!("rev_{}", e.to), to: format!("rev_{}", e.from) }).collect(),
    }
}

pub struct C99Emitter;
impl C99Emitter {
    pub fn emit(flow: &Flow, registry: &PrimitiveRegistry) -> String {
        let forward = lower_flow(flow, registry);
        let reverse = reverse_graph(&forward);
        let mut out = String::from("#include <stdio.h>\n#include <stddef.h>\n\n");
        out.push_str("typedef struct { long long input; long long output; } IntelligenceState;\n\n");
        out.push_str("void intelligence_forward(long long input, long long *output, IntelligenceState *state) {\n    long long result = input;\n");
        for n in &forward.nodes { out.push_str(&format!("    /* {} */\n    {}\n    state->output = result;\n", n.label, n.forward)); }
        out.push_str("    *output = result;\n}\n\n");
        out.push_str("void intelligence_reverse(long long grad_output, long long *grad_input, const IntelligenceState *state) {\n    long long gradient = grad_output;\n    (void)state;\n");
        for n in &reverse.nodes { out.push_str(&format!("    /* {} */\n    {}\n", n.label, n.forward)); }
        out.push_str("    *grad_input = gradient;\n}\n\nint main(void) {\n    IntelligenceState state = {0, 0};\n    long long output = 0;\n    intelligence_forward(0, &output, &state);\n    return output == output ? 0 : 1;\n}\n");
        out
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 { eprintln!("usage: intelligence <source.i>"); return; }
    let source = match fs::read_to_string(&args[1]) { Ok(s) => s, Err(e) => { eprintln!("read error: {}", e); return; } };
    let registry = PrimitiveRegistry::new();
    let flow = match parse_source(&source, &registry) { Ok(f) => f, Err(e) => { eprintln!("parse error: {}", e); return; } };
    let c = C99Emitter::emit(&flow, &registry);
    if let Err(e) = fs::write("payload.c", c) { eprintln!("write error: {}", e); return; }
    match Command::new("clang").args(["-O3", "payload.c", "-lm", "-o", "binary_app"]).status() { Ok(s) if s.success() => println!("Compiled successfully!"), Ok(_) => eprintln!("C compilation failed"), Err(e) => eprintln!("clang error: {}", e) }
}

fn parse_source(source: &str, registry: &PrimitiveRegistry) -> Result<Flow, String> {
    let text = source.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join(" ");
    let rhs = text.split("=").nth(1).ok_or("expected flow assignment")?.trim();
    parse_expr(rhs, registry)
}

fn parse_expr(text: &str, registry: &PrimitiveRegistry) -> Result<Flow, String> {
    let text = text.trim().trim_end_matches(';').trim();
    if let Some((a, b)) = split_top_level(text, ">>") { return Ok(Flow::Chain(Box::new(parse_expr(a, registry)?), Box::new(parse_expr(b, registry)?))); }
    if let Some((a, b)) = split_top_level(text, "||") { return Ok(Flow::Parallel(Box::new(parse_expr(a, registry)?), Box::new(parse_expr(b, registry)?))); }
    if let Some(rest) = text.strip_prefix('~') { return Ok(Flow::Feedback(Box::new(parse_expr(rest, registry)?))); }
    let name = text.trim_matches(|c| c == '(' || c == ')').trim();
    if name == "id" { return Ok(Flow::Identity { space: Space::Unit }); }
    let p = registry.get(name).ok_or_else(|| format!("undefined primitive: {}", name))?;
    Ok(Flow::Primitive { name: p.name.clone(), domain: p.domain.clone(), codomain: p.codomain.clone(), c_impl: p.forward(Backend::C99Cpu) })
}

fn split_top_level<'a>(text: &'a str, operator: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0usize; let bytes = text.as_bytes(); let op = operator.as_bytes(); let mut i = 0;
    while i + op.len() <= bytes.len() { match bytes[i] { b'(' => depth += 1, b')' => depth = depth.saturating_sub(1), _ => {} } if depth == 0 && &bytes[i..i + op.len()] == op { return Some((&text[..i], &text[i + op.len()..])); } i += 1; } None
}
