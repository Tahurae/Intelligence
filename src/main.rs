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
    ParameterShift,
    AdjointSensitivity,
    StraightThrough,
    NonDifferentiable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect { Pure, TensorDevice, QuantumDevice, BiologicalIo, HostIo }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Backend { C99Cpu, Simd, Cuda, OpenQasm, PulseFpga, RealtimeMea, Verilog }

#[derive(Debug, Clone, PartialEq)]
pub enum Flow {
    Identity { space: Space },
    Primitive { name: String, domain: Space, codomain: Space },
    Chain(Box<Flow>, Box<Flow>),
    Parallel(Box<Flow>, Box<Flow>),
    Feedback(Box<Flow>),
}

impl Flow {
    pub fn domain(&self) -> Space {
        match self {
            Flow::Identity { space } => space.clone(),
            Flow::Primitive { domain, .. } => domain.clone(),
            Flow::Chain(a, _) => a.domain(),
            Flow::Parallel(a, b) => Space::Product(Box::new(a.domain()), Box::new(b.domain())),
            Flow::Feedback(f) => f.domain(),
        }
    }

    pub fn codomain(&self) -> Space {
        match self {
            Flow::Identity { space } => space.clone(),
            Flow::Primitive { codomain, .. } => codomain.clone(),
            Flow::Chain(_, b) => b.codomain(),
            Flow::Parallel(a, b) => Space::Product(Box::new(a.codomain()), Box::new(b.codomain())),
            Flow::Feedback(f) => f.codomain(),
        }
    }

    pub fn grad(&self, registry: &PrimitiveRegistry) -> Flow {
        match self {
            Flow::Identity { space } => Flow::Identity { space: space.clone() },
            Flow::Primitive { name, domain, codomain } => Flow::Primitive {
                name: format!("grad_{}", name),
                domain: codomain.clone(),
                codomain: domain.clone(),
            },
            Flow::Chain(a, b) => Flow::Chain(Box::new(b.grad(registry)), Box::new(a.grad(registry))),
            Flow::Parallel(a, b) => Flow::Parallel(Box::new(a.grad(registry)), Box::new(b.grad(registry))),
            Flow::Feedback(f) => Flow::Feedback(Box::new(f.grad(registry))),
        }
    }
}

#[derive(Clone)]
pub struct Primitive {
    pub name: String,
    pub domain: Space,
    pub codomain: Space,
    pub derivative: DerivativeRule,
    pub effects: Vec<Effect>,
    pub backends: Vec<Backend>,
}

pub struct PrimitiveRegistry { primitives: HashMap<String, Primitive> }

impl PrimitiveRegistry {
    pub fn new() -> Self {
        let mut r = Self { primitives: HashMap::new() };
        r.add("add", Space::Scalar("Int".into()), Space::Scalar("Int".into()), DerivativeRule::Analytic, vec![Effect::Pure], vec![Backend::C99Cpu]);
        r.add("mul", Space::Scalar("Int".into()), Space::Scalar("Int".into()), DerivativeRule::Analytic, vec![Effect::Pure], vec![Backend::C99Cpu]);
        r.add("neural_layer", Space::Tensor { element: "Float32".into(), shape: vec![128] }, Space::Tensor { element: "Float32".into(), shape: vec![64] }, DerivativeRule::ReverseMode, vec![Effect::TensorDevice], vec![Backend::C99Cpu, Backend::Cuda, Backend::Simd]);
        r.add("quantum_gate", Space::Quantum { qubits: 6 }, Space::Quantum { qubits: 6 }, DerivativeRule::ParameterShift, vec![Effect::QuantumDevice], vec![Backend::OpenQasm, Backend::PulseFpga]);
        r.add("organoid_step", Space::Organoid { pins: 64 }, Space::Organoid { pins: 64 }, DerivativeRule::AdjointSensitivity, vec![Effect::BiologicalIo], vec![Backend::RealtimeMea, Backend::C99Cpu]);
        r.add("logic_step", Space::Logic { bits: 8 }, Space::Logic { bits: 8 }, DerivativeRule::StraightThrough, vec![Effect::Pure], vec![Backend::C99Cpu, Backend::Verilog]);
        r
    }

    fn add(&mut self, name: &str, domain: Space, codomain: Space, derivative: DerivativeRule, effects: Vec<Effect>, backends: Vec<Backend>) {
        self.primitives.insert(name.into(), Primitive { name: name.into(), domain, codomain, derivative, effects, backends });
    }

    fn register_declared(&mut self, name: String, domain: Space, codomain: Space) {
        let derivative = self.get(&name).map(|p| p.derivative.clone()).unwrap_or(DerivativeRule::Analytic);
        let effects = self.get(&name).map(|p| p.effects.clone()).unwrap_or_else(|| vec![Effect::Pure]);
        let backends = self.get(&name).map(|p| p.backends.clone()).unwrap_or_else(|| vec![Backend::C99Cpu]);
        self.add(&name, domain, codomain, derivative, effects, backends);
    }

    fn get(&self, name: &str) -> Option<&Primitive> { self.primitives.get(name) }
}

#[derive(Debug, Clone, PartialEq)]
enum Token { Ident(String), Number(usize), Space, Flow, Grad, Equals, Colon, Arrow, Chain, Parallel, Feedback, LParen, RParen, Lt, Gt, Comma }

struct Lexer<'a> { input: &'a str, pos: usize }
impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self { Self { input, pos: 0 } }
    fn next(&mut self) -> Option<Token> {
        while self.pos < self.input.len() && self.input[self.pos..].chars().next().unwrap().is_whitespace() { self.pos += self.input[self.pos..].chars().next().unwrap().len_utf8(); }
        if self.pos >= self.input.len() { return None; }
        let s = &self.input[self.pos..];
        if s.starts_with("//") { self.pos += s.find('\n').unwrap_or(s.len()); return self.next(); }
        for (text, token) in [(">>", Token::Chain), ("||", Token::Parallel), ("->", Token::Arrow)] {
            if s.starts_with(text) { self.pos += text.len(); return Some(token); }
        }
        let c = s.chars().next().unwrap();
        let single = match c { '=' => Some(Token::Equals), ':' => Some(Token::Colon), '~' => Some(Token::Feedback), '(' => Some(Token::LParen), ')' => Some(Token::RParen), '<' => Some(Token::Lt), '>' => Some(Token::Gt), ',' => Some(Token::Comma), _ => None };
        if let Some(t) = single { self.pos += c.len_utf8(); return Some(t); }
        if c.is_ascii_digit() { let n = s.chars().take_while(|x| x.is_ascii_digit()).map(char::len_utf8).sum(); let v = s[..n].parse().ok()?; self.pos += n; return Some(Token::Number(v)); }
        if c.is_alphanumeric() || c == '_' { let n = s.chars().take_while(|x| x.is_alphanumeric() || *x == '_').map(char::len_utf8).sum(); let word = &s[..n]; self.pos += n; return Some(match word { "space" => Token::Space, "flow" => Token::Flow, "grad" => Token::Grad, _ => Token::Ident(word.into()) }); }
        self.pos += c.len_utf8(); self.next()
    }
}

pub struct Parser { tokens: Vec<Token>, pos: usize, spaces: HashMap<String, Space>, flows: HashMap<String, Flow> }
impl Parser {
    pub fn new(source: &str) -> Self { let mut l = Lexer::new(source); let mut tokens = vec![]; while let Some(t) = l.next() { tokens.push(t); } Self { tokens, pos: 0, spaces: HashMap::new(), flows: HashMap::new() } }
    fn peek(&self) -> Option<&Token> { self.tokens.get(self.pos) }
    fn take(&mut self) -> Option<Token> { let t = self.tokens.get(self.pos).cloned(); if t.is_some() { self.pos += 1; } t }
    fn expect(&mut self, wanted: Token) -> Result<(), String> { if self.take() == Some(wanted) { Ok(()) } else { Err(format!("expected {:?} at token {}", wanted, self.pos)) } }

    pub fn parse(&mut self, registry: &mut PrimitiveRegistry) -> Result<Flow, String> {
        let mut last = None;
        while self.pos < self.tokens.len() {
            match self.peek() {
                Some(Token::Space) => self.space_decl()?,
                Some(Token::Flow) => { let (name, flow) = self.flow_decl(registry)?; self.flows.insert(name, flow.clone()); last = Some(flow); }
                _ => return Err(format!("unexpected token at top level: {:?}", self.peek())),
            }
        }
        last.ok_or_else(|| "no flow declaration found".into())
    }

    fn space_decl(&mut self) -> Result<(), String> {
        self.expect(Token::Space)?;
        let name = self.ident()?;
        if self.peek() == Some(&Token::Lt) { self.take(); while self.peek() != Some(&Token::Gt) { self.take().ok_or("unterminated space parameters")?; } self.expect(Token::Gt)?; }
        self.expect(Token::Equals)?;
        let value = self.space_expr()?;
        self.spaces.insert(name, value);
        Ok(())
    }

    fn flow_decl(&mut self, registry: &mut PrimitiveRegistry) -> Result<(String, Flow), String> {
        self.expect(Token::Flow)?;
        let name = self.ident()?;
        let declared = if self.peek() == Some(&Token::Colon) { self.take(); let d = self.space_expr()?; self.expect(Token::Arrow)?; let c = self.space_expr()?; Some((d, c)) } else { None };
        if self.peek() == Some(&Token::Equals) {
            self.take();
            let flow = self.expr(registry)?;
            self.validate(&flow, registry)?;
            if let Some((d, c)) = declared { if flow.domain() != d || flow.codomain() != c { return Err(format!("flow {} annotation does not match inferred type", name)); } }
            Ok((name, flow))
        } else {
            let (d, c) = declared.ok_or("primitive flow requires : Domain -> Codomain or = expression")?;
            registry.register_declared(name.clone(), d.clone(), c.clone());
            Ok((name.clone(), Flow::Primitive { name, domain: d, codomain: c }))
        }
    }

    fn space_expr(&mut self) -> Result<Space, String> {
        let name = self.ident()?;
        if self.peek() == Some(&Token::Lt) {
            self.take();
            let mut args = vec![];
            loop { match self.take() { Some(Token::Number(n)) => args.push(n), Some(Token::Ident(_)) => {}, _ => return Err("invalid generic space argument".into()) } if self.peek() != Some(&Token::Comma) { break; } self.take(); }
            self.expect(Token::Gt)?;
            let n = *args.first().unwrap_or(&0);
            return Ok(match name.as_str() { "Tensor" | "Array" => Space::Tensor { element: "Float32".into(), shape: vec![n] }, "Qubit" | "Quantum" => Space::Quantum { qubits: n }, "Organoid" | "MEA" => Space::Organoid { pins: n }, "Logic" => Space::Logic { bits: n }, _ => Space::Scalar(name) });
        }
        Ok(self.spaces.get(&name).cloned().unwrap_or(Space::Scalar(name)))
    }

    fn expr(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        let mut left = self.parallel(registry)?;
        while self.peek() == Some(&Token::Chain) { self.take(); left = Flow::Chain(Box::new(left), Box::new(self.parallel(registry)?)); }
        Ok(left)
    }
    fn parallel(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        let mut left = self.unary(registry)?;
        while self.peek() == Some(&Token::Parallel) { self.take(); left = Flow::Parallel(Box::new(left), Box::new(self.unary(registry)?)); }
        Ok(left)
    }
    fn unary(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> { if self.peek() == Some(&Token::Feedback) { self.take(); Ok(Flow::Feedback(Box::new(self.unary(registry)?))) } else { self.primary(registry) } }
    fn primary(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        match self.take() {
            Some(Token::LParen) => { let f = self.expr(registry)?; self.expect(Token::RParen)?; Ok(f) }
            Some(Token::Grad) => { self.expect(Token::LParen)?; let f = self.expr(registry)?; self.expect(Token::RParen)?; Ok(f.grad(registry)) }
            Some(Token::Ident(name)) => self.flows.get(&name).cloned().or_else(|| registry.get(&name).map(|p| Flow::Primitive { name: name.clone(), domain: p.domain.clone(), codomain: p.codomain.clone() })).ok_or_else(|| format!("undefined flow or primitive: {}", name)),
            other => Err(format!("expected flow expression, got {:?}", other)),
        }
    }
    fn ident(&mut self) -> Result<String, String> { match self.take() { Some(Token::Ident(s)) => Ok(s), _ => Err("expected identifier".into()) } }
    fn validate(&self, f: &Flow, registry: &PrimitiveRegistry) -> Result<(), String> {
        match f {
            Flow::Identity { .. } => Ok(()),
            Flow::Primitive { name, domain, codomain } => { let p = registry.get(name).ok_or_else(|| format!("undefined primitive {}", name))?; if &p.domain != domain || &p.codomain != codomain { return Err(format!("type mismatch in {}", name)); } Ok(()) },
            Flow::Chain(a, b) => { self.validate(a, registry)?; self.validate(b, registry)?; if a.codomain() != b.domain() { return Err(format!("chain mismatch: {:?} -> {:?}", a.codomain(), b.domain())); } Ok(()) },
            Flow::Parallel(a, b) => { self.validate(a, registry)?; self.validate(b, registry) },
            Flow::Feedback(a) => self.validate(a, registry),
        }
    }
}

pub struct C99Emitter;
impl C99Emitter {
    pub fn emit(flow: &Flow, registry: &PrimitiveRegistry) -> String {
        let mut c = String::from("#include <stdint.h>\n#include <stddef.h>\n#include <stdio.h>\n#include <string.h>\n\ntypedef struct { int quantum_fd; int mea_fd; } IntelligenceDevices;\n\n");
        c.push_str("static long long intelligence_add(long long x) { return x + 1; }\nstatic long long intelligence_mul(long long x) { return x * 2; }\n");
        c.push_str("static void neural_layer_forward(const float in[128], float out[64]) { for (size_t i=0;i<64;i++) { out[i]=0.0f; for(size_t j=0;j<128;j++) out[i] += in[j] * 0.0078125f; } }\n");
        c.push_str("static void neural_layer_reverse(const float grad_out[64], float grad_in[128]) { for(size_t j=0;j<128;j++){ grad_in[j]=0.0f; for(size_t i=0;i<64;i++) grad_in[j]+=grad_out[i]*0.0078125f; } }\n");
        c.push_str("static void quantum_gate_forward(IntelligenceDevices *d, const float in[64], float out[64]) { (void)d; memcpy(out,in,64*sizeof(float)); }\nstatic void quantum_gate_reverse(IntelligenceDevices *d, const float grad_out[64], float grad_in[64]) { (void)d; memcpy(grad_in,grad_out,64*sizeof(float)); }\n");
        c.push_str("static void organoid_step_forward(IntelligenceDevices *d, const float in[64], float out[64]) { (void)d; memcpy(out,in,64*sizeof(float)); }\nstatic void organoid_step_reverse(IntelligenceDevices *d, const float grad_out[64], float grad_in[64]) { (void)d; memcpy(grad_in,grad_out,64*sizeof(float)); }\n\n");
        c.push_str("void intelligence_forward(long long input, long long *output, IntelligenceDevices *devices) { long long result=input; (void)devices;\n");
        Self::emit_scalar(flow, registry, &mut c, "result");
        c.push_str("    *output=result; }\n\nvoid intelligence_reverse(long long grad_output, long long *grad_input, IntelligenceDevices *devices) { long long gradient=grad_output; (void)devices;\n");
        let reverse = flow.grad(registry);
        Self::emit_scalar(&reverse, registry, &mut c, "gradient");
        c.push_str("    *grad_input=gradient; }\n\nint main(void) { IntelligenceDevices d={0,0}; long long out=0; intelligence_forward(0,&out,&d); return 0; }\n");
        c
    }
    fn emit_scalar(f: &Flow, registry: &PrimitiveRegistry, out: &mut String, var: &str) {
        match f {
            Flow::Primitive { name, .. } if name == "add" => out.push_str(&format!("    {}=intelligence_add({});\n", var, var)),
            Flow::Primitive { name, .. } if name == "mul" => out.push_str(&format!("    {}=intelligence_mul({});\n", var, var)),
            Flow::Primitive { name, .. } => out.push_str(&format!("    /* {} requires its typed backend; scalar driver skipped */\n", name)),
            Flow::Identity { .. } => {},
            Flow::Chain(a,b) => { Self::emit_scalar(a,registry,out,var); Self::emit_scalar(b,registry,out,var); },
            Flow::Parallel(a,b) => { Self::emit_scalar(a,registry,out,var); Self::emit_scalar(b,registry,out,var); },
            Flow::Feedback(a) => Self::emit_scalar(a,registry,out,var),
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 { eprintln!("usage: intelligence <program.cat>"); return; }
    let source = match fs::read_to_string(&args[1]) { Ok(s) => s, Err(e) => { eprintln!("read error: {}",e); return; } };
    let mut registry = PrimitiveRegistry::new();
    let mut parser = Parser::new(&source);
    let flow = match parser.parse(&mut registry) { Ok(f) => f, Err(e) => { eprintln!("parse/type error: {}",e); return; } };
    let c = C99Emitter::emit(&flow, &registry);
    if let Err(e) = fs::write("payload.c", c) { eprintln!("write error: {}",e); return; }
    match Command::new("clang").args(["-std=c99","-O3","payload.c","-lm","-o","binary_app"]).status() { Ok(s) if s.success() => println!("Compiled successfully!"), Ok(_) => eprintln!("C compilation failed"), Err(e) => eprintln!("clang error: {}",e) }
}
