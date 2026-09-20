use std::collections::HashMap;
use std::env;
use std::fs;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Space {
    Unit,
    Scalar(String),
    Tensor { element: String, shape: Vec<String> },
    Quantum { qubits: usize },
    Organoid { pins: usize },
    Logic { bits: usize },
    Product(Box<Space>, Box<Space>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DerivativeRule {
    Analytic,
    ReverseMode,
    ParameterShift { shift: f32 },
    AdjointSensitivity,
    StraightThrough,
    Custom(String),
    NonDifferentiable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect {
    Pure,
    TensorDevice,
    QuantumDevice,
    BiologicalIo,
    HostIo,
    Allocation,
    Nondeterministic,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Backend {
    C99Cpu,
    Simd,
    Cuda,
    OpenQasm,
    PulseFpga,
    RealtimeMea,
    Verilog,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Flow {
    Identity { space: Space },
    Primitive {
        name: String,
        domain: Space,
        codomain: Space,
        c_impl: String,
    },
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
    pub fn default_impl(&self, backend: Backend) -> String {
        self.implementations
            .get(&backend)
            .cloned()
            .unwrap_or_else(|| "/* no implementation */".to_string())
    }
}

pub struct PrimitiveRegistry {
    primitives: HashMap<String, Primitive>,
}

impl PrimitiveRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            primitives: HashMap::new(),
        };

        registry.register(Primitive {
            name: "add".to_string(),
            domain: Space::Scalar("Int".to_string()),
            codomain: Space::Scalar("Int".to_string()),
            derivative: DerivativeRule::Analytic,
            effects: vec![Effect::Pure],
            implementations: HashMap::from([
                (Backend::C99Cpu, "result = a + b;".to_string()),
            ]),
        });

        registry.register(Primitive {
            name: "mul".to_string(),
            domain: Space::Scalar("Int".to_string()),
            codomain: Space::Scalar("Int".to_string()),
            derivative: DerivativeRule::Analytic,
            effects: vec![Effect::Pure],
            implementations: HashMap::from([
                (Backend::C99Cpu, "result = a * b;".to_string()),
            ]),
        });

        registry.register(Primitive {
            name: "neural_layer".to_string(),
            domain: Space::Tensor {
                element: "Float32".to_string(),
                shape: vec!["128".to_string()],
            },
            codomain: Space::Tensor {
                element: "Float32".to_string(),
                shape: vec!["64".to_string()],
            },
            derivative: DerivativeRule::ReverseMode,
            effects: vec![Effect::TensorDevice],
            implementations: HashMap::from([
                (Backend::C99Cpu, "// neural_layer forward\n".to_string()),
            ]),
        });

        registry.register(Primitive {
            name: "quantum_gate".to_string(),
            domain: Space::Quantum { qubits: 6 },
            codomain: Space::Quantum { qubits: 6 },
            derivative: DerivativeRule::ParameterShift { shift: 1.5707964 },
            effects: vec![Effect::QuantumDevice],
            implementations: HashMap::from([
                (Backend::OpenQasm, "// quantum_gate\n".to_string()),
            ]),
        });

        registry.register(Primitive {
            name: "organoid_step".to_string(),
            domain: Space::Organoid { pins: 64 },
            codomain: Space::Organoid { pins: 64 },
            derivative: DerivativeRule::AdjointSensitivity,
            effects: vec![Effect::BiologicalIo],
            implementations: HashMap::from([
                (Backend::RealtimeMea, "// organoid_step\n".to_string()),
            ]),
        });

        registry
    }

    pub fn register(&mut self, primitive: Primitive) {
        self.primitives.insert(primitive.name.clone(), primitive);
    }

    pub fn get(&self, name: &str) -> Option<&Primitive> {
        self.primitives.get(name)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    KwSpace,
    KwFlow,
    Ident(String),
    Number(usize),
    Equals,
    Colon,
    Arrow,
    OpChain,
    OpParallel,
    OpDagger,
    LParen,
    RParen,
    Lt,
    Gt,
    Comma,
}

struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn next_token(&mut self) -> Option<Token> {
        self.skip_whitespace();
        if self.pos >= self.input.len() {
            return None;
        }

        let rest = &self.input[self.pos..];

        if rest.starts_with("//") {
            if let Some(idx) = rest.find('\n') {
                self.pos += idx + 1;
                return self.next_token();
            }
            self.pos = self.input.len();
            return None;
        }

        if rest.starts_with(">>") {
            self.pos += 2;
            return Some(Token::OpChain);
        }
        if rest.starts_with("||") {
            self.pos += 2;
            return Some(Token::OpParallel);
        }
        if rest.starts_with("->") {
            self.pos += 2;
            return Some(Token::Arrow);
        }

        let ch = rest.chars().next().unwrap();
        match ch {
            '=' => {
                self.pos += 1;
                Some(Token::Equals)
            }
            ':' => {
                self.pos += 1;
                Some(Token::Colon)
            }
            '~' => {
                self.pos += 1;
                Some(Token::OpDagger)
            }
            '(' => {
                self.pos += 1;
                Some(Token::LParen)
            }
            ')' => {
                self.pos += 1;
                Some(Token::RParen)
            }
            '<' => {
                self.pos += 1;
                Some(Token::Lt)
            }
            '>' => {
                self.pos += 1;
                Some(Token::Gt)
            }
            ',' => {
                self.pos += 1;
                Some(Token::Comma)
            }
            _ if ch.is_ascii_digit() => {
                let len = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .map(char::len_utf8)
                    .sum();
                let digits = &rest[..len];
                self.pos += len;
                Some(Token::Number(digits.parse().unwrap()))
            }
            _ if ch.is_alphanumeric() || ch == '_' => {
                let len = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .map(char::len_utf8)
                    .sum();
                let ident = &rest[..len];
                self.pos += len;
                match ident {
                    "space" => Some(Token::KwSpace),
                    "flow" => Some(Token::KwFlow),
                    _ => Some(Token::Ident(ident.to_string())),
                }
            }
            _ => {
                self.pos += ch.len_utf8();
                self.next_token()
            }
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.input[self.pos..].chars().next().unwrap();
            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    space_defs: HashMap<String, Space>,
}

impl Parser {
    pub fn new(input: &str) -> Self {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();
        while let Some(tok) = lexer.next_token() {
            tokens.push(tok);
        }
        Self {
            tokens,
            pos: 0,
            space_defs: HashMap::new(),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let token = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(token)
        } else {
            None
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), String> {
        match self.advance() {
            Some(token) if token == expected => Ok(()),
            _ => Err("Syntax error".to_string()),
        }
    }

    pub fn parse(&mut self, registry: &mut PrimitiveRegistry) -> Result<Flow, String> {
        let mut main_flow = None;

        while self.pos < self.tokens.len() {
            match self.peek() {
                Some(Token::KwSpace) => {
                    self.parse_space_decl()?;
                }
                Some(Token::KwFlow) => {
                    main_flow = Some(self.parse_flow_decl(registry)?);
                }
                Some(Token::Ident(_)) => {
                    self.parse_primitive_decl(registry)?;
                }
                _ => {
                    self.advance();
                }
            }
        }

        main_flow.ok_or_else(|| "No flow found".to_string())
    }

    fn parse_space_decl(&mut self) -> Result<(), String> {
        self.expect(Token::KwSpace)?;
        let name = match self.advance() {
            Some(Token::Ident(name)) => name,
            _ => return Err("Expected space name".to_string()),
        };
        self.expect(Token::Equals)?;
        let space = self.parse_space_expr()?;
        self.space_defs.insert(name, space);
        Ok(())
    }

    fn parse_primitive_decl(&mut self, registry: &mut PrimitiveRegistry) -> Result<(), String> {
        let name = match self.advance() {
            Some(Token::Ident(name)) => name,
            _ => return Err("Expected primitive name".to_string()),
        };

        self.expect(Token::Colon)?;
        let domain = self.parse_space_expr()?;
        self.expect(Token::Arrow)?;
        let codomain = self.parse_space_expr()?;

        let c_impl = registry
            .get(&name)
            .map(|p| p.default_impl(Backend::C99Cpu))
            .unwrap_or_else(|| format!("/* {} not implemented */", name));

        registry.register(Primitive {
            name: name.clone(),
            domain: domain.clone(),
            codomain: codomain.clone(),
            derivative: DerivativeRule::Analytic,
            effects: vec![Effect::Pure],
            implementations: HashMap::from([(Backend::C99Cpu, c_impl)]),
        });

        Ok(())
    }

    fn parse_space_expr(&mut self) -> Result<Space, String> {
        match self.peek().cloned() {
            Some(Token::LParen) => {
                self.advance();
                let left = self.parse_space_expr()?;
                self.expect(Token::RParen)?;
                Ok(left)
            }
            Some(Token::Ident(name)) => {
                self.advance();
                match self.peek() {
                    Some(Token::Lt) => {
                        self.advance();
                        let shape_arg = match self.advance() {
                            Some(Token::Number(value)) => value,
                            Some(Token::Ident(ident)) => ident.parse::<usize>().unwrap_or(0),
                            _ => return Err("Expected dimension in generic space".to_string()),
                        };
                        self.expect(Token::Gt)?;

                        match name.as_str() {
                            "Tensor" | "Array" => Ok(Space::Tensor {
                                element: "Float32".to_string(),
                                shape: vec![shape_arg.to_string()],
                            }),
                            "Qubit" | "Quantum" => Ok(Space::Quantum { qubits: shape_arg }),
                            "Organoid" | "MEA" => Ok(Space::Organoid { pins: shape_arg }),
                            "Logic" => Ok(Space::Logic { bits: shape_arg }),
                            _ => Ok(Space::Scalar(name)),
                        }
                    }
                    _ => Ok(Space::Scalar(name)),
                }
            }
            Some(Token::Number(_)) => {
                let value = match self.advance() {
                    Some(Token::Number(v)) => v,
                    _ => return Err("Expected a number".to_string()),
                };
                Ok(Space::Scalar(value.to_string()))
            }
            _ => Err("Invalid space expression".to_string()),
        }
    }

    fn parse_flow_decl(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        self.expect(Token::KwFlow)?;
        let name = match self.advance() {
            Some(Token::Ident(name)) => name,
            _ => return Err("Expected flow name".to_string()),
        };

        let mut declared_domain = None;
        let mut declared_codomain = None;

        if matches!(self.peek(), Some(Token::Colon)) {
            self.advance();
            declared_domain = Some(self.parse_space_expr()?);
            self.expect(Token::Arrow)?;
            declared_codomain = Some(self.parse_space_expr()?);
        }

        self.expect(Token::Equals)?;
        let flow = self.parse_chain(registry)?;

        if let (Some(domain), Some(codomain)) = (declared_domain, declared_codomain) {
            let inferred = self.infer_codomain(&flow, registry)?;
            if inferred != codomain {
                return Err(format!(
                    "Declared codomain {:?} does not match inferred codomain {:?}",
                    codomain, inferred
                ));
            }
            let inferred_domain = self.infer_domain(&flow, registry)?;
            if inferred_domain != domain {
                return Err(format!(
                    "Declared domain {:?} does not match inferred domain {:?}",
                    domain, inferred_domain
                ));
            }
        }

        self.validate_flow(&flow, registry)?;

        let primitive_name = name.clone();
        let domain = self.infer_domain(&flow, registry)?;
        let codomain = self.infer_codomain(&flow, registry)?;

        let implementation = registry
            .get(&primitive_name)
            .map(|p| p.default_impl(Backend::C99Cpu))
            .unwrap_or_else(|| "/* generated flow */".to_string());

        Ok(Flow::Primitive {
            name,
            domain,
            codomain,
            c_impl: implementation,
        })
    }

    fn validate_flow(&self, flow: &Flow, registry: &PrimitiveRegistry) -> Result<Space, String> {
        match flow {
            Flow::Identity { space } => Ok(space.clone()),
            Flow::Primitive { name, domain, codomain, .. } => {
                let primitive = registry
                    .get(name)
                    .ok_or_else(|| format!("Undefined primitive: {}", name))?;
                if *domain != primitive.domain || *codomain != primitive.codomain {
                    return Err(format!(
                        "Type mismatch for {}: expected {:?} -> {:?}, got {:?} -> {:?}",
                        name, primitive.domain, primitive.codomain, domain, codomain
                    ));
                }
                Ok(codomain.clone())
            }
            Flow::Chain(first, second) => {
                let first_codomain = self.validate_flow(first, registry)?;
                let second_domain = self.infer_domain(second, registry)?;
                if first_codomain != second_domain {
                    return Err(format!(
                        "Chain type mismatch: {:?} != {:?}",
                        first_codomain, second_domain
                    ));
                }
                self.infer_codomain(second, registry)
            }
            Flow::Parallel(first, second) => {
                let first_type = self.validate_flow(first, registry)?;
                let second_type = self.validate_flow(second, registry)?;
                Ok(Space::Product(Box::new(first_type), Box::new(second_type)))
            }
            Flow::Feedback(inner) => {
                let inner_type = self.validate_flow(inner, registry)?;
                Ok(inner_type)
            }
        }
    }

    fn infer_domain(&self, flow: &Flow, registry: &PrimitiveRegistry) -> Result<Space, String> {
        match flow {
            Flow::Identity { space } => Ok(space.clone()),
            Flow::Primitive { domain, .. } => Ok(domain.clone()),
            Flow::Chain(first, _) => self.infer_domain(first, registry),
            Flow::Parallel(first, second) => Ok(Space::Product(
                Box::new(self.infer_domain(first, registry)?),
                Box::new(self.infer_domain(second, registry)?),
            )),
            Flow::Feedback(inner) => self.infer_domain(inner, registry),
        }
    }

    fn infer_codomain(&self, flow: &Flow, registry: &PrimitiveRegistry) -> Result<Space, String> {
        match flow {
            Flow::Identity { space } => Ok(space.clone()),
            Flow::Primitive { codomain, .. } => Ok(codomain.clone()),
            Flow::Chain(_, second) => self.infer_codomain(second, registry),
            Flow::Parallel(first, second) => Ok(Space::Product(
                Box::new(self.infer_codomain(first, registry)?),
                Box::new(self.infer_codomain(second, registry)?),
            )),
            Flow::Feedback(inner) => self.infer_codomain(inner, registry),
        }
    }

    fn parse_chain(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        let mut left = self.parse_parallel(registry)?;
        while matches!(self.peek(), Some(Token::OpChain)) {
            self.advance();
            let right = self.parse_parallel(registry)?;
            left = Flow::Chain(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_parallel(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        let mut left = self.parse_unary(registry)?;
        while matches!(self.peek(), Some(Token::OpParallel)) {
            self.advance();
            let right = self.parse_unary(registry)?;
            left = Flow::Parallel(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        if matches!(self.peek(), Some(Token::OpDagger)) {
            self.advance();
            Ok(Flow::Feedback(Box::new(self.parse_primary(registry)?)))
        } else {
            self.parse_primary(registry)
        }
    }

    fn parse_primary(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        match self.peek().cloned() {
            Some(Token::LParen) => {
                self.advance();
                let inner = self.parse_chain(registry)?;
                self.expect(Token::RParen)?;
                Ok(inner)
            }
            Some(Token::Ident(name)) => {
                self.advance();

                let primitive = registry
                    .get(&name)
                    .ok_or_else(|| format!("Undefined primitive: {}", name))?;

                Ok(Flow::Primitive {
                    name: primitive.name.clone(),
                    domain: primitive.domain.clone(),
                    codomain: primitive.codomain.clone(),
                    c_impl: primitive.default_impl(Backend::C99Cpu),
                })
            }
            _ => Err("Syntax error".to_string()),
        }
    }
}

pub struct C99Emitter;

impl C99Emitter {
    pub fn emit(flow: &Flow, registry: &PrimitiveRegistry) -> String {
        let mut code = String::new();
        code.push_str("#include <stdio.h>\n\n");
        code.push_str("int main(void) {\n");
        code.push_str("    long long result = 0;\n");
        Self::codegen(flow, &mut code, registry, "result");
        code.push_str("    return 0;\n");
        code.push_str("}\n");
        code
    }

    fn codegen(flow: &Flow, code: &mut String, registry: &PrimitiveRegistry, var: &str) {
        match flow {
            Flow::Identity { .. } => {}
            Flow::Primitive { name, c_impl, .. } => {
                code.push_str(&format!("    // Execute: {}\n", name));
                code.push_str(&format!("    {}\n", c_impl));
                code.push_str(&format!("    {} = result;\n", var));
            }
            Flow::Chain(first, second) => {
                Self::codegen(first, code, registry, var);
                Self::codegen(second, code, registry, var);
            }
            Flow::Parallel(first, second) => {
                code.push_str(&format!("    long long first_res = {};\n", var));
                code.push_str(&format!("    long long second_res = {};\n", var));
                Self::codegen(first, code, registry, "first_res");
                Self::codegen(second, code, registry, "second_res");
                code.push_str(&format!("    {} = first_res + second_res;\n", var));
            }
            Flow::Feedback(inner) => {
                Self::codegen(inner, code, registry, var);
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        return;
    }

    let code = match fs::read_to_string(&args[1]) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Failed to read file: {}", error);
            return;
        }
    };

    let mut registry = PrimitiveRegistry::new();
    let mut parser = Parser::new(&code);
    let ast = match parser.parse(&mut registry) {
        Ok(ast) => ast,
        Err(error) => {
            eprintln!("Parse error: {}", error);
            return;
        }
    };

    if let Err(error) = parser.validate_flow(&ast, &registry) {
        eprintln!("Type error: {}", error);
        return;
    }

    let c_code = C99Emitter::emit(&ast, &registry);
    if let Err(error) = fs::write("payload.c", c_code) {
        eprintln!("Failed to write payload.c: {}", error);
        return;
    }

    let status = match Command::new("clang")
        .args(["-O3", "payload.c", "-lm", "-o", "binary_app"])
        .status()
    {
        Ok(status) => status,
        Err(error) => {
            eprintln!("Failed to execute clang: {}", error);
            return;
        }
    };

    if status.success() {
        println!("Compiled successfully!");
    } else {
        eprintln!("C compilation failed");
    }
}
