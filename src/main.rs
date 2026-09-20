use std::collections::HashMap;
use std::env;
use std::fs;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Space {
    Base(String),
    Product(Box<Space>, Box<Space>),
    Identity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Flow {
    Identity,
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

pub struct PrimitiveRegistry {
    primitives: HashMap<String, (Space, Space, String)>,
}

impl PrimitiveRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            primitives: HashMap::new(),
        };

        registry.register("add", Space::Base("Int".into()), Space::Base("Int".into()), "long long result = a + b;".into());
        registry.register("mul", Space::Base("Int".into()), Space::Base("Int".into()), "long long result = a * b;".into());
        registry
    }

    pub fn register(&mut self, name: &str, dom: Space, cod: Space, implementation: String) {
        self.primitives.insert(name.to_string(), (dom, cod, implementation));
    }

    pub fn get(&self, name: &str) -> Option<(Space, Space, String)> {
        self.primitives.get(name).cloned()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    KwSpace,
    KwFlow,
    Ident(String),
    Equals,
    Colon,
    Arrow,
    OpChain,
    OpParallel,
    OpDagger,
    LParen,
    RParen,
}

struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self { Self { input, pos: 0 } }

    fn next_token(&mut self) -> Option<Token> {
        self.skip_whitespace();
        if self.pos >= self.input.len() { return None; }

        let rest = &self.input[self.pos..];
        if rest.starts_with("//") {
            if let Some(idx) = rest.find('\n') {
                self.pos += idx + 1;
                return self.next_token();
            }
            self.pos = self.input.len();
            return None;
        }
        if rest.starts_with(">>") { self.pos += 2; return Some(Token::OpChain); }
        if rest.starts_with("||") { self.pos += 2; return Some(Token::OpParallel); }
        if rest.starts_with("->") { self.pos += 2; return Some(Token::Arrow); }

        let ch = rest.chars().next().unwrap();
        match ch {
            '=' => { self.pos += 1; Some(Token::Equals) }
            ':' => { self.pos += 1; Some(Token::Colon) }
            '~' => { self.pos += 1; Some(Token::OpDagger) }
            '(' => { self.pos += 1; Some(Token::LParen) }
            ')' => { self.pos += 1; Some(Token::RParen) }
            _ if ch.is_alphanumeric() || ch == '_' => {
                let len = rest.chars()
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
            _ => { self.pos += ch.len_utf8(); self.next_token() }
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.input[self.pos..].chars().next().unwrap();
            if ch.is_whitespace() { self.pos += ch.len_utf8(); } else { break; }
        }
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    primitive_types: HashMap<String, (Space, Space)>,
}

impl Parser {
    pub fn new(input: &str) -> Self {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next_token() { tokens.push(token); }
        Self { tokens, pos: 0, primitive_types: HashMap::new() }
    }

    fn peek(&self) -> Option<&Token> { self.tokens.get(self.pos) }

    fn advance(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let token = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(token)
        } else { None }
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
                Some(Token::KwSpace) => self.parse_space_decl()?,
                Some(Token::KwFlow) => main_flow = Some(self.parse_flow_decl(registry)?),
                Some(Token::Ident(_)) => self.parse_primitive_decl(registry)?,
                _ => { self.advance(); }
            }
        }
        main_flow.ok_or_else(|| "No flow found".to_string())
    }

    fn parse_space_decl(&mut self) -> Result<(), String> {
        self.expect(Token::KwSpace)?;
        if matches!(self.peek(), Some(Token::Ident(_))) { self.advance(); }
        self.expect(Token::Equals)?;
        self.parse_space_expr()?;
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
        let c_impl = Self::get_default_impl(&name)
            .unwrap_or_else(|| format!("/* {} not implemented */", name));
        registry.register(&name, domain.clone(), codomain.clone(), c_impl);
        self.primitive_types.insert(name, (domain, codomain));
        Ok(())
    }

    fn get_default_impl(name: &str) -> Option<String> {
        match name {
            "add" => Some("long long result = a + b;".into()),
            "mul" => Some("long long result = a * b;".into()),
            "print" => Some("printf(\"%lld\\n\", x);".into()),
            _ => None,
        }
    }

    fn parse_space_expr(&mut self) -> Result<Space, String> {
        // Clone the token before mutating self. Matching directly on self.peek()
        // keeps an immutable borrow alive across self.advance().
        let mut left = match self.peek().cloned() {
            Some(Token::LParen) => {
                self.advance();
                let inner = self.parse_space_expr()?;
                self.expect(Token::RParen)?;
                inner
            }
            Some(Token::Ident(name)) => {
                self.advance();
                Space::Base(name)
            }
            _ => return Err("Invalid space expression".to_string()),
        };

        if matches!(self.peek(), Some(Token::OpParallel)) {
            self.advance();
            let right = self.parse_space_expr()?;
            left = Space::Product(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_flow_decl(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        self.expect(Token::KwFlow)?;
        if matches!(self.peek(), Some(Token::Ident(_))) { self.advance(); }
        self.expect(Token::Equals)?;
        let flow = self.parse_chain(registry)?;
        self.validate_flow(&flow, registry)?;
        Ok(flow)
    }

    fn validate_flow(&self, flow: &Flow, registry: &PrimitiveRegistry) -> Result<Space, String> {
        match flow {
            Flow::Identity => Ok(Space::Identity),
            Flow::Primitive { name, domain, codomain, .. } => {
                let (expected_domain, expected_codomain, _) = registry.get(name)
                    .ok_or_else(|| format!("Undefined primitive: {}", name))?;
                if *domain != expected_domain || *codomain != expected_codomain {
                    return Err(format!("Type mismatch for {}", name));
                }
                Ok(codomain.clone())
            }
            Flow::Chain(first, second) => {
                let first_codomain = self.validate_flow(first, registry)?;
                let second_domain = self.infer_domain(second, registry)?;
                if first_codomain != second_domain {
                    return Err(format!("Chain type mismatch: {:?} != {:?}", first_codomain, second_domain));
                }
                self.validate_flow(second, registry)
            }
            Flow::Parallel(first, second) => {
                let first_type = self.validate_flow(first, registry)?;
                let second_type = self.validate_flow(second, registry)?;
                Ok(Space::Product(Box::new(first_type), Box::new(second_type)))
            }
            Flow::Feedback(inner) => self.validate_flow(inner, registry),
        }
    }

    fn infer_domain(&self, flow: &Flow, registry: &PrimitiveRegistry) -> Result<Space, String> {
        match flow {
            Flow::Identity => Ok(Space::Identity),
            Flow::Primitive { domain, .. } => Ok(domain.clone()),
            Flow::Chain(first, _) => self.infer_domain(first, registry),
            Flow::Parallel(first, second) => Ok(Space::Product(
                Box::new(self.infer_domain(first, registry)?),
                Box::new(self.infer_domain(second, registry)?),
            )),
            Flow::Feedback(inner) => self.infer_domain(inner, registry),
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
                let (domain, codomain, c_impl) = registry.get(&name)
                    .or_else(|| self.primitive_types.get(&name).map(|(domain, codomain)| (
                        domain.clone(),
                        codomain.clone(),
                        Self::get_default_impl(&name).unwrap_or_else(|| format!("/* {} not implemented */", name)),
                    )))
                    .ok_or_else(|| format!("Undefined primitive: {}", name))?;
                Ok(Flow::Primitive { name, domain, codomain, c_impl })
            }
            _ => Err("Syntax error".to_string()),
        }
    }
}

pub struct C99Emitter;

impl C99Emitter {
    pub fn emit(flow: &Flow, registry: &PrimitiveRegistry) -> String {
        let mut code = String::from("#include <stdio.h>\n\nint main(void) {\n    long long result = 0;\n");
        Self::codegen(flow, &mut code, registry, "result");
        code.push_str("    return 0;\n}\n");
        code
    }

    fn codegen(flow: &Flow, code: &mut String, registry: &PrimitiveRegistry, var: &str) {
        match flow {
            Flow::Identity => {}
            Flow::Primitive { name, .. } => {
                if let Some((_, _, implementation)) = registry.get(name) {
                    code.push_str(&format!("    // Execute: {}\n    {}\n", name, implementation));
                }
            }
            Flow::Chain(first, second) => {
                Self::codegen(first, code, registry, var);
                Self::codegen(second, code, registry, var);
            }
            Flow::Parallel(first, second) => {
                code.push_str(&format!("    long long f_res = {};\n    long long g_res = {};\n", var, var));
                Self::codegen(first, code, registry, "f_res");
                Self::codegen(second, code, registry, "g_res");
                code.push_str(&format!("    {} = f_res + g_res;\n", var));
            }
            Flow::Feedback(inner) => Self::codegen(inner, code, registry, var),
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 { return; }

    let code = match fs::read_to_string(&args[1]) {
        Ok(code) => code,
        Err(error) => { eprintln!("Failed to read file: {}", error); return; }
    };

    let mut registry = PrimitiveRegistry::new();
    let mut parser = Parser::new(&code);
    let ast = match parser.parse(&mut registry) {
        Ok(ast) => ast,
        Err(error) => { eprintln!("Parse error: {}", error); return; }
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

    match Command::new("clang").args(["-O3", "payload.c", "-lm", "-o", "binary_app"]).status() {
        Ok(status) if status.success() => println!("Compiled successfully!"),
        Ok(_) => eprintln!("C compilation failed"),
        Err(error) => eprintln!("Failed to execute clang: {}", error),
    }
}
