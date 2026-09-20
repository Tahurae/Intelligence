use std::collections::HashMap;
use std::env;
use std::fs;
use std::process::Command;

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
    fn product(left: Space, right: Space) -> Space {
        let mut values = match left { Space::Product(v) => v, value => vec![value] };
        match right { Space::Product(v) => values.extend(v), value => values.push(value) }
        Space::Product(values)
    }

    fn flatten_product(&self) -> Vec<Space> {
        match self { Space::Product(values) => values.clone(), value => vec![value.clone()] }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimitiveFamily { Scalar, Tensor, Quantum, Organoid, Logic }

#[derive(Debug, Clone)]
pub struct Primitive {
    pub name: String,
    pub domain: Space,
    pub codomain: Space,
    pub family: PrimitiveFamily,
    pub forward_c: String,
    pub reverse_c: String,
}

#[derive(Debug, Clone)]
pub struct PrimitiveRegistry {
    pub primitives: HashMap<String, Primitive>,
    pub spaces: HashMap<String, Space>,
}

impl PrimitiveRegistry {
    fn new() -> Self {
        let mut registry = Self { primitives: HashMap::new(), spaces: HashMap::new() };
        registry.register(Primitive { name: "add".into(), domain: Space::Scalar("f64".into()), codomain: Space::Scalar("f64".into()), family: PrimitiveFamily::Scalar, forward_c: "*out = *in + 1.0;".into(), reverse_c: "*grad_in = *grad_out;".into() });
        registry.register(Primitive { name: "mul".into(), domain: Space::Scalar("f64".into()), codomain: Space::Scalar("f64".into()), family: PrimitiveFamily::Scalar, forward_c: "*out = *in * 2.0;".into(), reverse_c: "*grad_in = *grad_out * 2.0;".into() });
        registry.register(Primitive { name: "neural_layer".into(), domain: Space::Tensor(128), codomain: Space::Tensor(64), family: PrimitiveFamily::Tensor, forward_c: "tensor_forward((const float*)in, (float*)out, 128, 64);".into(), reverse_c: "tensor_reverse((const float*)in, (float*)out, 128, 64);".into() });
        registry.register(Primitive { name: "quantum_gate".into(), domain: Space::Qubit(2), codomain: Space::Qubit(2), family: PrimitiveFamily::Quantum, forward_c: "quantum_forward(dev, (const float*)in, (float*)out, 2);".into(), reverse_c: "quantum_reverse(dev, (const float*)in, (float*)out, 2);".into() });
        for name in ["organoid_step", "organoid_pulse"] {
            registry.register(Primitive { name: name.into(), domain: Space::Organoid(64), codomain: Space::Organoid(64), family: PrimitiveFamily::Organoid, forward_c: "organoid_forward(dev, (const float*)in, (float*)out, 64);".into(), reverse_c: "organoid_reverse(dev, (const float*)in, (float*)out, 64);".into() });
        }
        registry.register(Primitive { name: "logic_step".into(), domain: Space::Logic(8), codomain: Space::Logic(8), family: PrimitiveFamily::Logic, forward_c: "logic_forward((const uint8_t*)in, (uint8_t*)out, 8);".into(), reverse_c: "logic_reverse((const uint8_t*)in, (uint8_t*)out, 8);".into() });
        registry
    }

    fn register(&mut self, primitive: Primitive) {
        self.primitives.insert(primitive.name.clone(), primitive);
    }

    fn get(&self, name: &str) -> Option<&Primitive> { self.primitives.get(name) }

    fn declare_primitive(&mut self, name: String, domain: Space, codomain: Space) -> Result<(), String> {
        if self.primitives.contains_key(&name) {
            return Err(format!("primitive `{name}` is already registered"));
        }
        let family = family_for(&domain, &codomain);
        self.register(Primitive { name: name.clone(), domain, codomain, family, forward_c: format!("/* declared primitive {name} forward */"), reverse_c: format!("/* declared primitive {name} reverse */") });
        Ok(())
    }
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

impl Flow {
    fn infer(&self, registry: &PrimitiveRegistry, flows: &HashMap<String, Flow>) -> Result<(Space, Space), String> {
        match self {
            Flow::Primitive(name) => {
                if let Some(primitive) = registry.get(name) { Ok((primitive.domain.clone(), primitive.codomain.clone())) }
                else if let Some(flow) = flows.get(name) { flow.infer(registry, flows) }
                else { Err(format!("undefined primitive or flow `{name}`")) }
            }
            Flow::Identity(space) => Ok((space.clone(), space.clone())),
            Flow::Chain(first, second) => {
                let (first_domain, first_codomain) = first.infer(registry, flows)?;
                let (second_domain, second_codomain) = second.infer(registry, flows)?;
                if first_codomain != second_domain { return Err(format!("chain mismatch: {:?} cannot feed {:?}", first_codomain, second_domain)); }
                Ok((first_domain, second_codomain))
            }
            Flow::Parallel(left, right) => {
                let (left_domain, left_codomain) = left.infer(registry, flows)?;
                let (right_domain, right_codomain) = right.infer(registry, flows)?;
                Ok((Space::product(left_domain, right_domain), Space::product(left_codomain, right_codomain)))
            }
            Flow::Feedback(inner) => {
                let (domain, codomain) = inner.infer(registry, flows)?;
                let domain_parts = domain.flatten_product();
                let codomain_parts = codomain.flatten_product();
                if domain_parts.len() < 2 || codomain_parts.len() < 2 {
                    return Err("feedback requires exactly the shape f : A || U -> B || U".into());
                }
                let input_trace = domain_parts.last().unwrap();
                let output_trace = codomain_parts.last().unwrap();
                if input_trace != output_trace {
                    return Err(format!("feedback trace mismatch: input {:?}, output {:?}", input_trace, output_trace));
                }
                let public_domain = Space::Product(domain_parts[..domain_parts.len() - 1].to_vec());
                let public_codomain = Space::Product(codomain_parts[..codomain_parts.len() - 1].to_vec());
                Ok((collapse_product(public_domain), collapse_product(public_codomain)))
            }
            Flow::Derivative(inner) => {
                let (domain, codomain) = inner.infer(registry, flows)?;
                Ok((Space::product(domain, codomain), codomain))
            }
        }
    }

    fn grad(&self) -> Flow {
        match self {
            Flow::Primitive(name) => Flow::Primitive(format!("grad_{name}")),
            Flow::Chain(first, second) => Flow::Chain(Box::new(second.grad()), Box::new(first.grad())),
            Flow::Parallel(left, right) => Flow::Parallel(Box::new(left.grad()), Box::new(right.grad())),
            Flow::Feedback(inner) => Flow::Feedback(Box::new(inner.grad())),
            Flow::Identity(space) => Flow::Identity(space.clone()),
            Flow::Derivative(inner) => inner.grad(),
        }
    }
}

fn collapse_product(space: Space) -> Space {
    match space { Space::Product(mut values) if values.len() == 1 => values.remove(0), value => value }
}

#[derive(Debug, Clone, PartialEq)]
enum Token { Space, Flow, Grad, Id, Ident(String), Number(usize), Equals, Colon, Arrow, Chain, Parallel, Tilde, LParen, RParen, LAngle, RAngle, Comma, Eof }

struct Lexer<'a> { input: &'a [u8], position: usize }
impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self { Self { input: input.as_bytes(), position: 0 } }
    fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        while let Some(token) = self.next() { tokens.push(token); }
        tokens.push(Token::Eof);
        tokens
    }
    fn next(&mut self) -> Option<Token> {
        while self.position < self.input.len() && self.input[self.position].is_ascii_whitespace() { self.position += 1; }
        if self.position >= self.input.len() { return None; }
        if self.input[self.position..].starts_with(b"//") {
            while self.position < self.input.len() && self.input[self.position] != b'\n' { self.position += 1; }
            return self.next();
        }
        for (operator, token) in [(b">>".as_slice(), Token::Chain), (b"||".as_slice(), Token::Parallel), (b"->".as_slice(), Token::Arrow)] {
            if self.input[self.position..].starts_with(operator) { self.position += 2; return Some(token); }
        }
        let character = self.input[self.position];
        self.position += 1;
        Some(match character {
            b'=' => Token::Equals, b':' => Token::Colon, b'~' => Token::Tilde,
            b'(' => Token::LParen, b')' => Token::RParen, b'<' => Token::LAngle,
            b'>' => Token::RAngle, b',' => Token::Comma,
            c if c.is_ascii_digit() => { let mut value = (c - b'0') as usize; while self.position < self.input.len() && self.input[self.position].is_ascii_digit() { value = value * 10 + (self.input[self.position] - b'0') as usize; self.position += 1; } Token::Number(value) }
            c if c.is_ascii_alphabetic() || c == b'_' => { let start = self.position - 1; while self.position < self.input.len() && (self.input[self.position].is_ascii_alphanumeric() || self.input[self.position] == b'_') { self.position += 1; } let word = std::str::from_utf8(&self.input[start..self.position]).unwrap(); match word { "space" => Token::Space, "flow" => Token::Flow, "grad" => Token::Grad, "id" => Token::Id, _ => Token::Ident(word.into()) } }
            _ => return self.next(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct FlowDecl { pub name: String, pub domain: Option<Space>, pub codomain: Option<Space>, pub body: Flow }

pub struct Parser { tokens: Vec<Token>, position: usize, pub flows: HashMap<String, Flow> }
impl Parser {
    fn new(tokens: Vec<Token>) -> Self { Self { tokens, position: 0, flows: HashMap::new() } }
    fn peek(&self) -> &Token { &self.tokens[self.position] }
    fn take(&mut self) -> Token { let token = self.tokens[self.position].clone(); if self.position + 1 < self.tokens.len() { self.position += 1; } token }
    fn expect(&mut self, expected: Token) -> Result<(), String> { let actual = self.take(); if actual == expected { Ok(()) } else { Err(format!("expected {:?}, got {:?}", expected, actual)) } }
    fn ident(&mut self) -> Result<String, String> { match self.take() { Token::Ident(value) => Ok(value), token => Err(format!("expected identifier, got {:?}", token)) } }

    fn parse(&mut self, registry: &mut PrimitiveRegistry) -> Result<Vec<FlowDecl>, String> {
        let mut declarations = Vec::new();
        while *self.peek() != Token::Eof {
            match self.peek() {
                Token::Space => self.parse_space(registry)?,
                Token::Flow => { let declaration = self.parse_flow(registry)?; if self.flows.contains_key(&declaration.name) { return Err(format!("flow `{}` is already declared", declaration.name)); } self.flows.insert(declaration.name.clone(), declaration.body.clone()); declarations.push(declaration); }
                Token::Ident(_) => self.parse_primitive(registry)?,
                token => return Err(format!("unexpected top-level token {:?}", token)),
            }
        }
        Ok(declarations)
    }

    fn parse_space(&mut self, registry: &mut PrimitiveRegistry) -> Result<(), String> {
        self.take();
        let name = self.ident()?;
        self.expect(Token::Equals)?;
        let value = self.parse_space_spec(registry)?;
        if registry.spaces.insert(name.clone(), value).is_some() { return Err(format!("space `{name}` is already declared")); }
        Ok(())
    }

    fn parse_primitive(&mut self, registry: &mut PrimitiveRegistry) -> Result<(), String> {
        let name = self.ident()?;
        self.expect(Token::Colon)?;
        let domain = self.parse_space_spec(registry)?;
        self.expect(Token::Arrow)?;
        let codomain = self.parse_space_spec(registry)?;
        registry.declare_primitive(name, domain, codomain)
    }

    fn parse_space_spec(&mut self, registry: &PrimitiveRegistry) -> Result<Space, String> {
        match self.take() {
            Token::Ident(name) => {
                if let Some(space) = registry.spaces.get(&name) { return Ok(space.clone()); }
                if self.peek() == &Token::LAngle {
                    self.take();
                    let size = match self.take() { Token::Number(value) => value, token => return Err(format!("expected dimension, got {:?}", token)) };
                    self.expect(Token::RAngle)?;
                    return Ok(match name.as_str() { "Tensor" | "Array" => Space::Tensor(size), "Qubit" | "Quantum" => Space::Qubit(size), "Organoid" | "MEA" => Space::Organoid(size), "Logic" | "Bits" => Space::Logic(size), _ => return Err(format!("unknown parameterized space `{name}`")) });
                }
                Err(format!("unknown space `{name}`"))
            }
            Token::LParen => {
                let mut components = vec![self.parse_space_spec(registry)?];
                while self.peek() == &Token::Parallel { self.take(); components.push(self.parse_space_spec(registry)?); }
                self.expect(Token::RParen)?;
                if components.len() < 2 { return Err("product space requires at least two components".into()); }
                Ok(Space::Product(components))
            }
            token => Err(format!("expected space specification, got {:?}", token)),
        }
    }

    fn parse_flow(&mut self, registry: &PrimitiveRegistry) -> Result<FlowDecl, String> {
        self.take();
        let name = self.ident()?;
        let (domain, codomain) = if self.peek() == &Token::Colon { self.take(); let domain = self.parse_space_spec(registry)?; self.expect(Token::Arrow)?; let codomain = self.parse_space_spec(registry)?; (Some(domain), Some(codomain)) } else { (None, None) };
        self.expect(Token::Equals)?;
        let body = self.parse_expr(registry)?;
        let inferred = body.infer(registry, &self.flows)?;
        if let (Some(expected_domain), Some(expected_codomain)) = (&domain, &codomain) { if inferred != (expected_domain.clone(), expected_codomain.clone()) { return Err(format!("flow `{name}` annotation mismatch: inferred {:?}, expected {:?}", inferred, (expected_domain, expected_codomain))); } }
        Ok(FlowDecl { name, domain, codomain, body })
    }

    fn parse_expr(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> { let mut value = self.parse_parallel(registry)?; while self.peek() == &Token::Chain { self.take(); value = Flow::Chain(Box::new(value), Box::new(self.parse_parallel(registry)?)); } Ok(value) }
    fn parse_parallel(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> { let mut value = self.parse_primary(registry)?; while self.peek() == &Token::Parallel { self.take(); value = Flow::Parallel(Box::new(value), Box::new(self.parse_primary(registry)?)); } Ok(value) }
    fn parse_primary(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        match self.take() {
            Token::Tilde => Ok(Flow::Feedback(Box::new(self.parse_primary(registry)?))),
            Token::Grad => { self.expect(Token::LParen)?; let flow = self.parse_expr(registry)?; self.expect(Token::RParen)?; Ok(Flow::Derivative(Box::new(flow))) }
            Token::Id => { if self.peek() == &Token::LParen { self.take(); let space = self.parse_space_spec(registry)?; self.expect(Token::RParen)?; Ok(Flow::Identity(space)) } else { Ok(Flow::Identity(Space::Unit)) } }
            Token::Ident(name) => Ok(self.flows.get(&name).cloned().unwrap_or(Flow::Primitive(name))),
            Token::LParen => { let flow = self.parse_expr(registry)?; self.expect(Token::RParen)?; Ok(flow) }
            token => Err(format!("unexpected flow token {:?}", token)),
        }
    }
}

struct Emitter<'a> { registry: &'a PrimitiveRegistry }
impl<'a> Emitter<'a> {
    fn emit(&self, declarations: &[FlowDecl]) -> String {
        let mut code = String::from("#include <stdint.h>\n#include <stddef.h>\n#include <stdio.h>\n#include <string.h>\n\ntypedef struct { int qasm_fd; int mea_fd; } IntelligenceDevices;\n\n");
        code.push_str("static void tensor_forward(const float *in,float *out,size_t n,size_t m){for(size_t i=0;i<m;i++){out[i]=0.0f;for(size_t j=0;j<n;j++)out[i]+=in[j]*0.01f;}}\n");
        code.push_str("static void tensor_reverse(const float *out,float *in,size_t n,size_t m){for(size_t i=0;i<n;i++){in[i]=0.0f;for(size_t j=0;j<m;j++)in[i]+=out[j]*0.01f;}}\n");
        code.push_str("static void quantum_forward(IntelligenceDevices *d,const float *in,float *out,size_t q){(void)d;memcpy(out,in,(1u<<q)*2*sizeof(float));}\n");
        code.push_str("static void quantum_reverse(IntelligenceDevices *d,const float *out,float *in,size_t q){(void)d;memcpy(in,out,(1u<<q)*2*sizeof(float));}\n");
        code.push_str("static void organoid_forward(IntelligenceDevices *d,const float *in,float *out,size_t n){(void)d;memcpy(out,in,n*sizeof(float));}\n");
        code.push_str("static void organoid_reverse(IntelligenceDevices *d,const float *out,float *in,size_t n){(void)d;memcpy(in,out,n*sizeof(float));}\n");
        code.push_str("static void logic_forward(const uint8_t *in,uint8_t *out,size_t bits){memcpy(out,in,(bits+7)/8);}\n");
        code.push_str("static void logic_reverse(const uint8_t *out,uint8_t *in,size_t bits){memcpy(in,out,(bits+7)/8);}\n\n");
        for declaration in declarations { code.push_str(&format!("/* flow {} : {:?} -> {:?} */\nvoid {}_forward(IntelligenceDevices *dev,const void *in,void *out){{\n", declaration.name, declaration.domain, declaration.codomain, declaration.name)); self.emit_flow(&declaration.body, &mut code, "in", "out", false); code.push_str("}\n\n"); code.push_str(&format!("void {}_reverse(IntelligenceDevices *dev,const void *in,void *out){{\n", declaration.name)); self.emit_flow(&declaration.body, &mut code, "in", "out", true); code.push_str("}\n\n"); }
        code.push_str("int main(void){IntelligenceDevices dev={0,0};(void)dev;puts(\"Intelligence categorical runtime ready\");return 0;}\n");
        code
    }

    fn emit_flow(&self, flow: &Flow, code: &mut String, input: &str, output: &str, reverse: bool) {
        match flow {
            Flow::Primitive(name) => {
                if let Some(primitive) = self.registry.get(name) { code.push_str("    "); code.push_str(if reverse { &primitive.reverse_c } else { &primitive.forward_c }); code.push('\n'); }
                else { code.push_str(&format!("    /* unresolved primitive `{name}` */\n")); }
            }
            Flow::Identity(_) => code.push_str(&format!("    memcpy({}, {}, sizeof(void*));\n", output, input)),
            Flow::Chain(first, second) => { code.push_str("    /* chained composition */\n"); self.emit_flow(first, code, input, output, reverse); self.emit_flow(second, code, input, output, reverse); }
            Flow::Parallel(left, right) => { code.push_str("    /* product wiring: branch buffers are backend-specific */\n"); self.emit_flow(left, code, input, output, reverse); self.emit_flow(right, code, input, output, reverse); }
            Flow::Feedback(inner) => { code.push_str("    /* traced feedback */\n"); self.emit_flow(inner, code, input, output, reverse); }
            Flow::Derivative(inner) => self.emit_flow(inner, code, input, output, true),
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).map(String::as_str).unwrap_or("program.cat");
    let source = match fs::read_to_string(path) { Ok(value) => value, Err(error) => { eprintln!("read error: {error}"); return; } };
    let mut registry = PrimitiveRegistry::new();
    let mut lexer = Lexer::new(&source);
    let mut parser = Parser::new(lexer.tokenize());
    let declarations = match parser.parse(&mut registry) { Ok(value) => value, Err(error) => { eprintln!("parse/type error: {error}"); return; } };
    for declaration in &declarations { if let Err(error) = declaration.body.infer(&registry, &parser.flows) { eprintln!("type error in {}: {error}", declaration.name); return; } }
    let generated = Emitter { registry: &registry }.emit(&declarations);
    if let Err(error) = fs::write("payload.c", generated) { eprintln!("write error: {error}"); return; }
    match Command::new("clang").args(["-std=c99", "-O2", "payload.c", "-o", "binary_app"]).status() { Ok(status) if status.success() => println!("Compiled successfully: binary_app"), Ok(_) => eprintln!("C compilation failed; payload.c was preserved"), Err(error) => eprintln!("clang error: {error}; payload.c was preserved") }
}
