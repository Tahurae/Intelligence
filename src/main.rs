use std::{collections::HashMap, env, fs, process::Command};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Space {
    Scalar(String),
    Tensor(usize),
    Qubit(usize),
    Organoid(usize),
    Logic(usize),
    Product(Vec<Space>),
    Unit,
}

impl Space {
    fn product(a: Space, b: Space) -> Space {
        let mut out = match a { Space::Product(v) => v, x => vec![x] };
        match b { Space::Product(v) => out.extend(v), x => out.push(x) }
        Space::Product(out)
    }
    fn c_scalar(&self) -> &'static str { "double" }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimitiveFamily { Scalar, Tensor, Quantum, Organoid, Logic }

#[derive(Debug, Clone)]
pub struct Primitive {
    pub name: String,
    pub domain: Space,
    pub codomain: Space,
    pub family: PrimitiveFamily,
}

#[derive(Debug, Clone)]
pub struct PrimitiveRegistry {
    primitives: HashMap<String, Primitive>,
    spaces: HashMap<String, Space>,
}

impl PrimitiveRegistry {
    fn new() -> Self {
        let mut r = Self { primitives: HashMap::new(), spaces: HashMap::new() };
        r.register("add", Space::Scalar("f64".into()), Space::Scalar("f64".into()));
        r.register("mul", Space::Scalar("f64".into()), Space::Scalar("f64".into()));
        r.register("neural_layer", Space::Tensor(128), Space::Tensor(64));
        r.register("quantum_gate", Space::Qubit(2), Space::Qubit(2));
        r.register("organoid_step", Space::Organoid(64), Space::Organoid(64));
        r.register("organoid_pulse", Space::Organoid(64), Space::Organoid(64));
        r.register("logic_step", Space::Logic(8), Space::Logic(8));
        r
    }
    fn register(&mut self, name: &str, domain: Space, codomain: Space) {
        let family = family_for(&domain, &codomain);
        self.primitives.insert(name.into(), Primitive { name: name.into(), domain, codomain, family });
    }
    fn declare_primitive(&mut self, name: String, domain: Space, codomain: Space) -> Result<(), String> {
        if self.primitives.contains_key(&name) { return Err(format!("primitive `{name}` is already declared")); }
        self.register(&name, domain, codomain);
        Ok(())
    }
    fn primitive(&self, name: &str) -> Option<&Primitive> { self.primitives.get(name) }
}

fn family_for(domain: &Space, codomain: &Space) -> PrimitiveFamily {
    match (domain, codomain) {
        (Space::Tensor(_), _) | (_, Space::Tensor(_)) => PrimitiveFamily::Tensor,
        (Space::Qubit(_), _) | (_, Space::Qubit(_)) => PrimitiveFamily::Quantum,
        (Space::Organoid(_), _) | (_, Space::Organoid(_)) => PrimitiveFamily::Organoid,
        (Space::Logic(_), _) | (_, Space::Logic(_)) => PrimitiveFamily::Logic,
        _ => PrimitiveFamily::Scalar,
    }
}

#[derive(Debug, Clone)]
pub enum Flow {
    Primitive(String),
    Chain(Box<Flow>, Box<Flow>),
    Parallel(Box<Flow>, Box<Flow>),
    Feedback(Box<Flow>),
    Identity(Space),
    Derivative(Box<Flow>),
}

#[derive(Debug, Clone)]
pub struct FlowDecl { name: String, domain: Option<Space>, codomain: Option<Space>, body: Flow }

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Space, Flow, Grad, Id, Ident(String), Number(usize), Equals, Colon,
    Arrow, Chain, Parallel, Tilde, LParen, RParen, LAngle, RAngle, Comma, Eof,
}

struct Lexer<'a> { bytes: &'a [u8], pos: usize }
impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self { Self { bytes: source.as_bytes(), pos: 0 } }
    fn tokenize(&mut self) -> Vec<Token> {
        let mut result = Vec::new();
        while let Some(t) = self.next() { result.push(t); }
        result.push(Token::Eof); result
    }
    fn next(&mut self) -> Option<Token> {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() { self.pos += 1; }
        if self.pos >= self.bytes.len() { return None; }
        if self.bytes[self.pos..].starts_with(b"//") {
            while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' { self.pos += 1; }
            return self.next();
        }
        for (text, token) in [(b">>".as_slice(), Token::Chain), (b"||".as_slice(), Token::Parallel), (b"->".as_slice(), Token::Arrow)] {
            if self.bytes[self.pos..].starts_with(text) { self.pos += 2; return Some(token); }
        }
        let c = self.bytes[self.pos]; self.pos += 1;
        Some(match c {
            b'=' => Token::Equals, b':' => Token::Colon, b'~' => Token::Tilde,
            b'(' => Token::LParen, b')' => Token::RParen, b'<' => Token::LAngle,
            b'>' => Token::RAngle, b',' => Token::Comma,
            c if c.is_ascii_digit() => {
                let mut n = (c - b'0') as usize;
                while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() { n = n * 10 + (self.bytes[self.pos] - b'0') as usize; self.pos += 1; }
                Token::Number(n)
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = self.pos - 1;
                while self.pos < self.bytes.len() && (self.bytes[self.pos].is_ascii_alphanumeric() || self.bytes[self.pos] == b'_') { self.pos += 1; }
                match std::str::from_utf8(&self.bytes[start..self.pos]).unwrap() {
                    "space" => Token::Space, "flow" => Token::Flow, "grad" => Token::Grad, "id" => Token::Id,
                    s => Token::Ident(s.into()),
                }
            }
            _ => return self.next(),
        })
    }
}

pub struct Parser { tokens: Vec<Token>, pos: usize, flows: HashMap<String, Flow> }
impl Parser {
    fn new(tokens: Vec<Token>) -> Self { Self { tokens, pos: 0, flows: HashMap::new() } }
    fn peek(&self) -> &Token { &self.tokens[self.pos] }
    fn take(&mut self) -> Token { let t = self.tokens[self.pos].clone(); if self.pos + 1 < self.tokens.len() { self.pos += 1; } t }
    fn expect(&mut self, expected: Token) -> Result<(), String> { let actual = self.take(); if actual == expected { Ok(()) } else { Err(format!("expected {:?}, got {:?}", expected, actual)) } }
    fn ident(&mut self) -> Result<String, String> { match self.take() { Token::Ident(s) => Ok(s), t => Err(format!("expected identifier, got {:?}", t)) } }

    fn parse(&mut self, registry: &mut PrimitiveRegistry) -> Result<Vec<FlowDecl>, String> {
        let mut declarations = Vec::new();
        while *self.peek() != Token::Eof {
            match self.peek() {
                Token::Space => self.parse_space(registry)?,
                Token::Flow => {
                    let decl = self.parse_flow(registry)?;
                    if self.flows.insert(decl.name.clone(), decl.body.clone()).is_some() { return Err(format!("flow `{}` is already declared", decl.name)); }
                    declarations.push(decl);
                }
                Token::Ident(_) => self.parse_primitive(registry)?,
                t => return Err(format!("unexpected top-level token {:?}", t)),
            }
        }
        Ok(declarations)
    }

    fn parse_space(&mut self, registry: &mut PrimitiveRegistry) -> Result<(), String> {
        self.take(); let name = self.ident()?; self.expect(Token::Equals)?;
        let space = self.space_spec(registry)?;
        if registry.spaces.insert(name.clone(), space).is_some() { return Err(format!("space `{name}` is already declared")); }
        Ok(())
    }

    fn parse_primitive(&mut self, registry: &mut PrimitiveRegistry) -> Result<(), String> {
        let name = self.ident()?; self.expect(Token::Colon)?;
        let domain = self.space_spec(registry)?; self.expect(Token::Arrow)?;
        let codomain = self.space_spec(registry)?;
        registry.declare_primitive(name, domain, codomain)
    }

    fn space_spec(&mut self, registry: &PrimitiveRegistry) -> Result<Space, String> {
        match self.take() {
            Token::Ident(name) => {
                if let Some(alias) = registry.spaces.get(&name) { return Ok(alias.clone()); }
                let dimension = match self.peek() {
                    Token::LAngle => { self.take(); let n = match self.take() { Token::Number(n) => n, t => return Err(format!("expected integer dimension, got {:?}", t)) }; self.expect(Token::RAngle)?; Some(n) }
                    _ => None,
                };
                match (name.as_str(), dimension) {
                    ("Tensor" | "Array", Some(n)) => Ok(Space::Tensor(n)),
                    ("Qubit" | "Quantum", Some(n)) => Ok(Space::Qubit(n)),
                    ("Organoid" | "MEA", Some(n)) => Ok(Space::Organoid(n)),
                    ("Logic" | "Bits", Some(n)) => Ok(Space::Logic(n)),
                    ("Tensor" | "Array" | "Qubit" | "Quantum" | "Organoid" | "MEA" | "Logic" | "Bits", None) => Err(format!("space `{name}` requires an explicit dimension, e.g. {name}<N>")),
                    (_, Some(_)) => Err(format!("unknown parameterized space `{name}`")),
                    (_, None) => Err(format!("undeclared space `{name}`")),
                }
            }
            Token::LParen => {
                let mut parts = vec![self.space_spec(registry)?];
                while self.peek() == &Token::Parallel { self.take(); parts.push(self.space_spec(registry)?); }
                self.expect(Token::RParen)?;
                if parts.len() < 2 { return Err("product space requires at least two components".into()); }
                Ok(Space::Product(parts))
            }
            t => Err(format!("expected space specification, got {:?}", t)),
        }
    }

    fn parse_flow(&mut self, registry: &PrimitiveRegistry) -> Result<FlowDecl, String> {
        self.take(); let name = self.ident()?;
        let (domain, codomain) = if self.peek() == &Token::Colon { self.take(); let d = self.space_spec(registry)?; self.expect(Token::Arrow)?; let c = self.space_spec(registry)?; (Some(d), Some(c)) } else { (None, None) };
        self.expect(Token::Equals)?;
        let body = self.expr(registry)?;
        let inferred = body.infer(registry, &self.flows)?;
        if let (Some(d), Some(c)) = (&domain, &codomain) {
            if inferred != (d.clone(), c.clone()) { return Err(format!("flow `{name}` annotation mismatch: inferred {:?}, expected {:?}", inferred, (d, c))); }
        }
        Ok(FlowDecl { name, domain, codomain, body })
    }

    fn expr(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> { let mut f = self.parallel(registry)?; while self.peek() == &Token::Chain { self.take(); f = Flow::Chain(Box::new(f), Box::new(self.parallel(registry)?)); } Ok(f) }
    fn parallel(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> { let mut f = self.primary(registry)?; while self.peek() == &Token::Parallel { self.take(); f = Flow::Parallel(Box::new(f), Box::new(self.primary(registry)?)); } Ok(f) }
    fn primary(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        match self.take() {
            Token::Tilde => Ok(Flow::Feedback(Box::new(self.primary(registry)?))),
            Token::Grad => { self.expect(Token::LParen)?; let f = self.expr(registry)?; self.expect(Token::RParen)?; Ok(Flow::Derivative(Box::new(f))) }
            Token::Id => { if self.peek() == &Token::LParen { self.take(); let s = self.space_spec(registry)?; self.expect(Token::RParen)?; Ok(Flow::Identity(s)) } else { Ok(Flow::Identity(Space::Unit)) } }
            Token::Ident(name) => Ok(self.flows.get(&name).cloned().unwrap_or(Flow::Primitive(name))),
            Token::LParen => { let f = self.expr(registry)?; self.expect(Token::RParen)?; Ok(f) }
            t => Err(format!("unexpected flow token {:?}", t)),
        }
    }
}

impl Flow {
    fn infer(&self, registry: &PrimitiveRegistry, flows: &HashMap<String, Flow>) -> Result<(Space, Space), String> {
        match self {
            Flow::Primitive(name) => registry.primitive(name).map(|p| (p.domain.clone(), p.codomain.clone())).or_else(|| flows.get(name).and_then(|f| f.infer(registry, flows).ok())).ok_or_else(|| format!("undefined primitive or flow `{name}`")),
            Flow::Identity(s) => Ok((s.clone(), s.clone())),
            Flow::Chain(a, b) => { let (ad, ac) = a.infer(registry, flows)?; let (bd, bc) = b.infer(registry, flows)?; if ac != bd { return Err(format!("chain mismatch: {:?} cannot feed {:?}", ac, bd)); } Ok((ad, bc)) }
            Flow::Parallel(a, b) => { let (ad, ac) = a.infer(registry, flows)?; let (bd, bc) = b.infer(registry, flows)?; Ok((Space::product(ad, bd), Space::product(ac, bc))) }
            Flow::Feedback(inner) => {
                let (d, c) = inner.infer(registry, flows)?;
                let dp = match d { Space::Product(v) if v.len() >= 2 => v, _ => return Err("feedback requires f : A || U -> B || U".into()) };
                let cp = match c { Space::Product(v) if v.len() >= 2 => v, _ => return Err("feedback requires f : A || U -> B || U".into()) };
                let du = dp.last().unwrap(); let cu = cp.last().unwrap();
                if du != cu { return Err(format!("feedback trace mismatch: {:?} != {:?}", du, cu)); }
                Ok((collapse(Space::Product(dp[..dp.len()-1].to_vec())), collapse(Space::Product(cp[..cp.len()-1].to_vec()))))
            }
            Flow::Derivative(inner) => { let (d, c) = inner.infer(registry, flows)?; Ok((Space::product(d, c.clone()), d)) }
        }
    }
}

fn collapse(s: Space) -> Space { match s { Space::Product(mut v) if v.len() == 1 => v.remove(0), x => x } }

struct C99Emitter<'a> { registry: &'a PrimitiveRegistry }
impl<'a> C99Emitter<'a> {
    fn emit(&self, declarations: &[FlowDecl]) -> String {
        let mut c = String::from("#include <stdint.h>\n#include <stddef.h>\n#include <stdio.h>\n#include <string.h>\n\ntypedef struct { int qasm_fd; int mea_fd; } IntelligenceDevices;\n\n");
        c.push_str("static void tensor_forward(const float*i,float*o,size_t n,size_t m){for(size_t x=0;x<m;x++){o[x]=0;for(size_t y=0;y<n;y++)o[x]+=i[y]*0.01f;}}\nstatic void tensor_reverse(const float*o,float*i,size_t n,size_t m){for(size_t x=0;x<n;x++){i[x]=0;for(size_t y=0;y<m;y++)i[x]+=o[y]*0.01f;}}\nstatic void quantum_forward(IntelligenceDevices*d,const float*i,float*o,size_t q){(void)d;memcpy(o,i,(1u<<q)*2*sizeof(float));}\nstatic void quantum_reverse(IntelligenceDevices*d,const float*o,float*i,size_t q){(void)d;memcpy(i,o,(1u<<q)*2*sizeof(float));}\nstatic void organoid_forward(IntelligenceDevices*d,const float*i,float*o,size_t n){(void)d;memcpy(o,i,n*sizeof(float));}\nstatic void organoid_reverse(IntelligenceDevices*d,const float*o,float*i,size_t n){(void)d;memcpy(i,o,n*sizeof(float));}\nstatic void logic_forward(const uint8_t*i,uint8_t*o,size_t n){memcpy(o,i,(n+7)/8);}\nstatic void logic_reverse(const uint8_t*o,uint8_t*i,size_t n){memcpy(i,o,(n+7)/8);}\n\n");
        for d in declarations { c.push_str(&format!("/* {} : {:?} -> {:?} */\n", d.name, d.domain, d.codomain)); self.emit_flow(&d.body, &mut c, false); }
        c.push_str("int main(void){IntelligenceDevices dev={0,0};(void)dev;puts(\"Intelligence categorical runtime ready\");return 0;}\n"); c
    }
    fn emit_flow(&self, flow: &Flow, c: &mut String, reverse: bool) {
        match flow {
            Flow::Primitive(name) => if let Some(p) = self.registry.primitive(name) { c.push_str(&format!("/* {} {:?} {} */\n", name, p.family, if reverse { "reverse" } else { "forward" })); },
            Flow::Chain(a,b) | Flow::Parallel(a,b) => { self.emit_flow(a,c,reverse); self.emit_flow(b,c,reverse); }
            Flow::Feedback(f) => self.emit_flow(f,c,reverse),
            Flow::Identity(_) => {},
            Flow::Derivative(f) => self.emit_flow(f,c,true),
        }
    }
}

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| "program.cat".into());
    let source = match fs::read_to_string(&path) { Ok(s) => s, Err(e) => { eprintln!("read error: {e}"); return; } };
    let mut registry = PrimitiveRegistry::new();
    let mut lexer = Lexer::new(&source);
    let mut parser = Parser::new(lexer.tokenize());
    let declarations = match parser.parse(&mut registry) { Ok(d) => d, Err(e) => { eprintln!("parse error: {e}"); return; } };
    for declaration in &declarations {
        let inferred = match declaration.body.infer(&registry, &parser.flows) { Ok(t) => t, Err(e) => { eprintln!("type error in {}: {e}", declaration.name); return; } };
        if let (Some(d), Some(c)) = (&declaration.domain, &declaration.codomain) && inferred != (d.clone(), c.clone()) { eprintln!("type annotation mismatch in {}", declaration.name); return; }
    }
    let output = C99Emitter { registry: &registry }.emit(&declarations);
    if let Err(e) = fs::write("payload.c", output) { eprintln!("write error: {e}"); return; }
    match Command::new("clang").args(["-std=c99", "-O2", "payload.c", "-o", "binary_app"]).status() { Ok(s) if s.success() => println!("Compiled successfully: binary_app"), Ok(_) => eprintln!("C compilation failed; payload.c preserved"), Err(e) => eprintln!("clang error: {e}; payload.c preserved") }
}
