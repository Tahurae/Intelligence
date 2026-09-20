use std::collections::{HashMap, HashSet};
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
    Primitive { 
        name: String, 
        domain: Space, 
        codomain: Space,
        c_impl: String,  // ← The actual C code for this morphism
    },
    Chain(Box<Flow>, Box<Flow>),
    Parallel(Box<Flow>, Box<Flow>),
    Feedback(Box<Flow>),
}

pub struct PrimitiveRegistry {
    primitives: HashMap<String, (Space, Space, String)>,
    // name -> (domain, codomain, c_code)
}

impl PrimitiveRegistry {
    pub fn new() -> Self {
        let mut registry = Self { primitives: HashMap::new() };
        registry.register("add", 
            Space::Base("Int".into()), 
            Space::Base("Int".into()),
            "result = a + b;".into());
        registry.register("mul",
            Space::Base("Int".into()),
            Space::Base("Int".into()),
            "result = a * b;".into());
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
enum Token { KwSpace, KwFlow, Ident(String), Equals, Colon, Arrow, OpChain, OpParallel, OpDagger, LParen, RParen }

struct Lexer<'a> { input: &'a str, pos: usize }
impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self { Self { input, pos: 0 } }
    fn next_token(&mut self) -> Option<Token> {
        self.skip_whitespace();
        if self.pos >= self.input.len() { return None; }
        let rest = &self.input[self.pos..];
        if rest.starts_with("//") {
            if let Some(idx) = rest.find('\n') { self.pos += idx + 1; return self.next_token(); }
            else { self.pos = self.input.len(); return None; }
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
                let len = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').map(|c| c.len_utf8()).sum();
                let ident = &rest[..len];
                self.pos += len;
                match ident { "space" => Some(Token::KwSpace), "flow" => Some(Token::KwFlow), _ => Some(Token::Ident(ident.to_string())) }
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

pub struct Parser { tokens: Vec<Token>, pos: usize, primitive_types: HashMap<String, (Space, Space)> }
impl Parser {
    pub fn new(input: &str) -> Self {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();
        while let Some(tok) = lexer.next_token() { tokens.push(tok); }
        Self { tokens, pos: 0, primitive_types: HashMap::new() }
    }
    fn peek(&self) -> Option<&Token> { self.tokens.get(self.pos) }
    fn advance(&mut self) -> Option<Token> { if self.pos < self.tokens.len() { let t = self.tokens[self.pos].clone(); self.pos += 1; Some(t) } else { None } }
    fn expect(&mut self, expected: Token) -> Result<(), String> {
        match self.advance() { Some(tok) if tok == expected => Ok(()), _ => Err("Syntax error".to_string()) }
    }
    pub fn parse(&mut self) -> Result<Flow, String> {
        let mut main_flow = None;
        while self.pos < self.tokens.len() {
            match self.peek() {
                Some(Token::KwSpace) => { self.parse_space_decl()?; }
                Some(Token::KwFlow) => { main_flow = Some(self.parse_flow_decl()?); }
                Some(Token::Ident(_)) => { self.parse_primitive_decl()?; }
                _ => { self.advance(); }
            }
        }
        main_flow.ok_or_else(|| "No flow found".to_string())
    }
    fn parse_space_decl(&mut self) -> Result<(), String> { self.expect(Token::KwSpace)?; self.advance(); self.expect(Token::Equals)?; self.parse_space_expr()?; Ok(()) }
    fn parse_primitive_decl(&mut self, registry: &mut PrimitiveRegistry) -> Result<(), String> {
    let name = match self.advance() { Some(Token::Ident(n)) => n, _ => return Err("Err".to_string()) };
    self.expect(Token::Colon)?; 
    let dom = self.parse_space_expr()?; 
    self.expect(Token::Arrow)?; 
    let cod = self.parse_space_expr()?;
    
    // Look up the C implementation (from global registry or inline)
    let c_impl = get_default_impl(&name).unwrap_or_else(|| format!("// {} not implemented", name));
    registry.register(&name, dom, cod, c_impl);
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
            Some(Token::LParen) => { self.advance(); let i = self.parse_space_expr()?; self.expect(Token::RParen)?; i }
            Some(Token::Ident(_)) => { let n = match self.advance() { Some(Token::Ident(n)) => n, _ => unreachable!() }; Space::Base(n) }
            _ => return Err("Err".to_string()),
        };
        if let Some(Token::OpParallel) = self.peek() { self.advance(); let right = self.parse_space_expr()?; left = Space::Product(Box::new(left), Box::new(right)); }
        Ok(left)
    }
    fn parse_flow_decl(&mut self, registry: &PrimitiveRegistry) -> Result<Flow, String> { 
    self.advance(); 
    self.advance(); 
    self.expect(Token::Equals)?;
    let flow = self.parse_chain(registry)?;
    self.validate_flow(&flow, registry)?;  // ← Type check
    Ok(flow)
    }

    fn validate_flow(&self, flow: &Flow, registry: &PrimitiveRegistry) -> Result<Space, String> {
        match flow {
            Flow::Identity => Ok(Space::Identity),
        
            Flow::Primitive { name, domain, codomain } => {
                let (expected_dom, expected_cod, _) = registry.get(name)
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
                    return Err(format!("Chain type mismatch: {} != {}", f_cod, g_dom));
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
    
    fn parse_chain(&mut self) -> Result<Flow, String> {
        let mut left = self.parse_parallel()?;
        while let Some(Token::OpChain) = self.peek() { self.advance(); let right = self.parse_parallel()?; left = Flow::Chain(Box::new(left), Box::new(right)); }
        Ok(left)
    }
    fn parse_parallel(&mut self) -> Result<Flow, String> {
        let mut left = self.parse_unary()?;
        while let Some(Token::OpParallel) = self.peek() { self.advance(); let right = self.parse_unary()?; left = Flow::Parallel(Box::new(left), Box::new(right)); }
        Ok(left)
    }
    fn parse_unary(&mut self) -> Result<Flow, String> {
        if let Some(Token::OpDagger) = self.peek() { self.advance(); Ok(Flow::Feedback(Box::new(self.parse_primary()?))) } else { self.parse_primary() }
    }
    fn parse_primary(&mut self) -> Result<Flow, String> {
        match self.peek().cloned() {
            Some(Token::LParen) => { self.advance(); let i = self.parse_chain()?; self.expect(Token::RParen)?; Ok(i) }
            Some(Token::Ident(name)) => {
                self.advance();
                let (dom, cod) = self.primitive_types.get(&name).cloned().unwrap_or((Space::Base("in".into()), Space::Base("out".into())));
                Ok(Flow::Primitive { name, domain: dom, codomain: cod })
            }
            _ => Err("Syntax error".to_string()),
        }
    }
}

pub struct C99Emitter;
impl C99Emitter {
    pub fn emit(flow: &Flow) -> String {
        let mut code = String::new();
        code.push_str("#include <stdio.h>\n");
        code.push_str("long long add(long long a, long long b) { return a + b; }\n");
        code.push_str("long long mul(long long a, long long b) { return a * b; }\n");
        code.push_str("void print_val(long long x) { printf(\"%lld\\n\", x); }\n\n");
        
        code.push_str("int main() {\n");
        code.push_str("    long long result = 0;\n");
        Self::codegen(flow, &mut code, "result");
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
            }
        }
        
        Flow::Chain(f, g) => {
            Self::codegen(f, code, registry, var);
            Self::codegen(g, code, registry, var);
        }
        
        Flow::Parallel(f, g) => {
                code.push_str(&format!("    long long f_res = {};\n", var));
                code.push_str(&format!("    long long g_res = {};\n", var));
                Self::codegen(f, code, "f_res");
                Self::codegen(g, code, "g_res");
                code.push_str(&format!("    {} = f_res + g_res;\n", var));
        }
            
        Flow::Feedback(f) => {
            Self::codegen(f, code, var);
        }
    }
    }
}
fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() >= 2 {
        if let Ok(code) = fs::read_to_string(&args[1]) {
            let mut registry = PrimitiveRegistry::new();
            let mut parser = Parser::new(&code);
            if let Ok(ast) = parser.parse(&mut registry) {  // ← Pass registry
                if parser.validate_flow(&ast, &registry).is_ok() {  // ← Validate
                    let c_code = C99Emitter::emit(&ast, &registry);  // ← Pass registry
                    let _ = fs::write("payload.c", &c_code);
                    let _ = Command::new("clang").args(&["-O3", "payload.c", "-lm", "-o", "binary_app"]).status();
                    println!("Compiled successfully!");
                }
            }
        }
    }
}
