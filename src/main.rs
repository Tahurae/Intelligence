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

        registry.register(
            "add",
            Space::Base("Int".into()),
            Space::Base("Int".into()),
            "result = a + b;".into(),
        );
        registry.register(
            "mul",
            Space::Base("Int".into()),
            Space::Base("Int".into()),
            "result = a * b;".into(),
        );
        registry
    }

    pub fn register(&mut self, name: &str, dom: Space, cod: Space, impl_: String) {
        self.primitives.insert(name.to_string(), (dom, cod, impl_));
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
            } else {
                self.pos = self.input.len();
                return None;
            }
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
            _ if ch.is_alphanumeric() || ch == '_' => {
                let len = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .map(|c| c.len_utf8())
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
    primitive_types: HashMap<String, (Space, Space)>,
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
            primitive_types: HashMap::new(),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let t = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(t)
        } else {
            None
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), String> {
        match self.advance() {
            Some(tok) if tok == expected => Ok(()),
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
                _ => {
                    self.advance();
                }
            }
        }

        main_flow.ok_or_else(|| "No flow found".to_string())
    }

    fn parse_space_decl(&mut self) -> Result<(), String> {
        self.expect(Token::KwSpace)?;
        if matches!(self.peek(), Some(Token::Ident(_))) {
            self.advance();
        }
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

        registry.register(&name, domain.clone(), codomain.clone(), c_impl.clone());
        self.primitive_types.insert(name.clone(), (domain.clone(), codomain.clone()));

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
        let mut left = match self.peek() {
            Some(Token::LParen) => {
                self.advance();
                let inner = self.parse_space_expr()?;
                self.expect(Token::RParen)?;
                inner
            }
            Some(Token::Ident(name)) => {
                self.advance();
                Space::Base(name.clone())
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
        if matches!(self.peek(), Some(Token::Ident(_))) {
            self.advance();
        }
        self.expect(Token::Equals)?;

        let flow = self.parse_chain(registry)?;
        self.validate_flow(&flow, registry)?;
        Ok(flow)
    }

    fn validate_flow(&self, flow: &Flow, registry: &PrimitiveRegistry) -> Result<Space, String> {
        match flow {
            Flow::Identity => Ok(Space::Identity),
            Flow::Primitive {
                name,
                domain,
                codomain,
                ..
            } => {
                let (expected_dom, expected_cod, _) = registry
                    .get(name)
                    .ok_or_else(|| format!("Undefined primitive: {}", name))?;

                if *domain != expected_dom || *codomain != expected_cod {
                    return Err(format!("Type mismatch for {}", name));
                }

                Ok(codomain.clone())
            }
            Flow::Chain(f, g) => {
                let f_cod = self.validate_flow(f, registry)?;
                let g_dom = self.infer_domain(g, registry)?;

                if f_cod != g_dom {
                    return Err(format!(
                        "Chain type mismatch: {:?} != {:?}",
                        f_cod, g_dom
                    ));
                }

                self.validate_flow(g, registry)
            }
            Flow::Parallel(f, g) => {
                let f_type = self.validate_flow(f, registry)?;
                let g_type = self.validate_flow(g, registry)?;
                Ok(Space::Product(Box::new(f_type), Box::new(g_type)))
            }
            Flow::Feedback(f) => self.validate_flow(f, registry),
        }
    }

    fn infer_domain(&self, flow: &Flow, registry: &PrimitiveRegistry) -> Result<Space, String> {
        match flow {
            Flow::Identity => Ok(Space::Identity),
            Flow::Primitive { domain, .. } => Ok(domain.clone()),
            Flow::Chain(f, _) => self.infer_domain(f, registry),
            Flow::Parallel(f, g) => {
                let f_domain = self.infer_domain(f, registry)?;
                let g_domain = self.infer_domain(g, registry)?;
                Ok(Space::Product(Box::new(f_domain), Box::new(g_domain)))
            }
            Flow::Feedback(f) => self.infer_domain(f, registry),
        }
    }

    fn parse_chain(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        let mut left = self.parse_parallel(registry)?;

        while let Some(Token::OpChain) = self.peek() {
            self.advance();
            let right = self.parse_parallel(registry)?;
            left = Flow::Chain(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_parallel(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> {
        let mut left = self.parse_unary(registry)?;

        while let Some(Token::OpParallel) = self.peek() {
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

                let (domain, codomain, c_impl) = registry
                    .get(&name)
                    .or_else(|| {
                        self.primitive_types.get(&name).map(|(dom, cod)| {
                            (
                                dom.clone(),
                                cod.clone(),
                                Self::get_default_impl(&name)
                                    .unwrap_or_else(|| format!("/* {} not implemented */", name)),
                            )
                        })
                    })
                    .ok_or_else(|| format!("Undefined primitive: {}", name))?;

                Ok(Flow::Primitive {
                    name,
                    domain,
                    codomain,
                    c_impl,
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
        code.push_str("#include <stdio.h>\n");
        code.push_str("#include <stdlib.h>\n\n");
        code.push_str("long long add(long long a, long long b) { return a + b; }\n");
        code.push_str("long long mul(long long a, long long b) { return a * b; }\n");
        code.push_str("void print_val(long long x) { printf(\"%lld\\n\", x); }\n\n");
        code.push_str("int main(void) {\n");
        code.push_str("    long long result = 0;\n");

        Self::codegen(flow, &mut code, registry, "result");

        code.push_str("    return 0;\n");
        code.push_str("}\n");
        code
    }

    fn codegen(flow: &Flow, code: &mut String, registry: &PrimitiveRegistry, var: &str) {
        match flow {
            Flow::Identity => {}
            Flow::Primitive { name, .. } => {
                if let Some((_, _, impl_)) = registry.get(name) {
                    code.push_str(&format!("    // Execute: {}\n", name));
                    code.push_str(&format!("    {}\n", impl_));
                } else {
                    code.push_str(&format!("    // {} not implemented\n", name));
                }
            }
            Flow::Chain(f, g) => {
                Self::codegen(f, code, registry, var);
                Self::codegen(g, code, registry, var);
            }
            Flow::Parallel(f, g) => {
                code.push_str(&format!("    long long f_res = {};\n", var));
                code.push_str(&format!("    long long g_res = {};\n", var));
                Self::codegen(f, code, registry, "f_res");
                Self::codegen(g, code, registry, "g_res");
                code.push_str(&format!("    {} = f_res + g_res;\n", var));
            }
            Flow::Feedback(f) => {
                Self::codegen(f, code, registry, var);
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
        Err(err) => {
            eprintln!("Failed to read file: {}", err);
            return;
        }
    };

    let mut registry = PrimitiveRegistry::new();
    let mut parser = Parser::new(&code);

    let ast = match parser.parse(&mut registry) {
        Ok(ast) => ast,
        Err(err) => {
            eprintln!("Parse error: {}", err);
            return;
        }
    };

    if let Err(err) = parser.validate_flow(&ast, &registry) {
        eprintln!("Type error: {}", err);
        return;
    }

    let c_code = C99Emitter::emit(&ast, &registry);
    if let Err(err) = fs::write("payload.c", &c_code) {
        eprintln!("Failed to write payload.c: {}", err);
        return;
    }

    let status = match Command::new("clang")
        .args(["-O3", "payload.c", "-lm", "-o", "binary_app"])
        .status()
    {
        Ok(status) => status,
        Err(err) => {
            eprintln!("Failed to execute clang: {}", err);
            return;
        }
    };

    if status.success() {
        println!("Compiled successfully!");
    } else {
        eprintln!("C compilation failed");
    }
}
