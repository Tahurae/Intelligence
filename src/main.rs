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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerivativeRule { Analytic, ReverseMode, ParameterShift, AdjointSensitivity, StraightThrough, NonDifferentiable }

#[derive(Debug, Clone, PartialEq, Eq)]
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
        match self { Self::Identity { space } => space.clone(), Self::Primitive { domain, .. } => domain.clone(), Self::Chain(a, _) => a.domain(), Self::Parallel(a,b) => Space::Product(Box::new(a.domain()), Box::new(b.domain())), Self::Feedback(f) => f.domain() }
    }
    pub fn codomain(&self) -> Space {
        match self { Self::Identity { space } => space.clone(), Self::Primitive { codomain, .. } => codomain.clone(), Self::Chain(_,b) => b.codomain(), Self::Parallel(a,b) => Space::Product(Box::new(a.codomain()), Box::new(b.codomain())), Self::Feedback(f) => f.codomain() }
    }
    pub fn grad(&self, registry: &PrimitiveRegistry) -> Flow {
        match self {
            Self::Identity { space } => Self::Identity { space: space.clone() },
            Self::Primitive { name, domain, codomain } => Self::Primitive { name: registry.reverse_name(name), domain: codomain.clone(), codomain: domain.clone() },
            Self::Chain(a,b) => Self::Chain(Box::new(b.grad(registry)), Box::new(a.grad(registry))),
            Self::Parallel(a,b) => Self::Parallel(Box::new(a.grad(registry)), Box::new(b.grad(registry))),
            Self::Feedback(f) => Self::Feedback(Box::new(f.grad(registry))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Primitive { pub name: String, pub domain: Space, pub codomain: Space, pub derivative: DerivativeRule, pub backends: Vec<Backend> }

pub struct PrimitiveRegistry { primitives: HashMap<String, Primitive> }
impl PrimitiveRegistry {
    pub fn new() -> Self {
        let mut r = Self { primitives: HashMap::new() };
        r.add("add", Space::Scalar("Int".into()), Space::Scalar("Int".into()), DerivativeRule::Analytic, vec![Backend::C99Cpu]);
        r.add("mul", Space::Scalar("Int".into()), Space::Scalar("Int".into()), DerivativeRule::Analytic, vec![Backend::C99Cpu]);
        r.add("neural_layer", Space::Tensor { element: "Float32".into(), shape: vec![128] }, Space::Tensor { element: "Float32".into(), shape: vec![64] }, DerivativeRule::ReverseMode, vec![Backend::C99Cpu, Backend::Cuda, Backend::Simd]);
        r.add("quantum_gate", Space::Quantum { qubits: 6 }, Space::Quantum { qubits: 6 }, DerivativeRule::ParameterShift, vec![Backend::OpenQasm, Backend::PulseFpga]);
        r.add("organoid_step", Space::Organoid { pins: 64 }, Space::Organoid { pins: 64 }, DerivativeRule::AdjointSensitivity, vec![Backend::RealtimeMea, Backend::C99Cpu]);
        r.add("organoid_pulse", Space::Organoid { pins: 64 }, Space::Organoid { pins: 64 }, DerivativeRule::AdjointSensitivity, vec![Backend::RealtimeMea, Backend::C99Cpu]);
        r.add("logic_step", Space::Logic { bits: 8 }, Space::Logic { bits: 8 }, DerivativeRule::StraightThrough, vec![Backend::C99Cpu, Backend::Verilog]);
        r
    }
    fn add(&mut self, name: &str, domain: Space, codomain: Space, derivative: DerivativeRule, backends: Vec<Backend>) { self.primitives.insert(name.into(), Primitive { name: name.into(), domain, codomain, derivative, backends }); }
    fn get(&self, name: &str) -> Option<&Primitive> { self.primitives.get(name) }
    fn reverse_name(&self, name: &str) -> String { if name.starts_with("grad_") { name.into() } else { format!("grad_{}", name) } }
    fn declare(&mut self, name: String, domain: Space, codomain: Space) { let (d,b) = self.get(&name).map(|p| (p.derivative.clone(), p.backends.clone())).unwrap_or((DerivativeRule::Analytic, vec![Backend::C99Cpu])); self.add(&name, domain, codomain, d, b); }
}

#[derive(Debug, Clone, PartialEq)]
enum Token { Ident(String), Number(usize), Space, Flow, Grad, Equals, Colon, Arrow, Chain, Parallel, Feedback, LParen, RParen, Lt, Gt, Comma }
struct Lexer<'a> { source: &'a str, pos: usize }
impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self { Self { source, pos: 0 } }
    fn next(&mut self) -> Option<Token> {
        while self.pos < self.source.len() { let c = self.source[self.pos..].chars().next().unwrap(); if c.is_whitespace() { self.pos += c.len_utf8(); } else { break; } }
        if self.pos >= self.source.len() { return None; }
        let s = &self.source[self.pos..];
        if s.starts_with("//") { self.pos += s.find('\n').unwrap_or(s.len()); return self.next(); }
        for (op,t) in [(">>",Token::Chain),("||",Token::Parallel),("->",Token::Arrow)] { if s.starts_with(op) { self.pos += op.len(); return Some(t); } }
        let c = s.chars().next().unwrap();
        let t = match c { '='=>Some(Token::Equals), ':'=>Some(Token::Colon), '~'=>Some(Token::Feedback), '('=>Some(Token::LParen), ')'=>Some(Token::RParen), '<'=>Some(Token::Lt), '>'=>Some(Token::Gt), ','=>Some(Token::Comma), _=>None };
        if let Some(t) = t { self.pos += c.len_utf8(); return Some(t); }
        if c.is_ascii_digit() { let n=s.chars().take_while(|x|x.is_ascii_digit()).map(char::len_utf8).sum(); self.pos+=n; return Some(Token::Number(s[..n].parse().ok()?)); }
        if c.is_alphanumeric() || c=='_' { let n=s.chars().take_while(|x|x.is_alphanumeric()||*x=='_').map(char::len_utf8).sum(); let w=&s[..n]; self.pos+=n; return Some(match w { "space"=>Token::Space, "flow"=>Token::Flow, "grad"=>Token::Grad, _=>Token::Ident(w.into()) }); }
        self.pos += c.len_utf8(); self.next()
    }
}

pub struct Parser { tokens: Vec<Token>, pos: usize, spaces: HashMap<String, Space>, flows: HashMap<String, Flow> }
impl Parser {
    pub fn new(source: &str) -> Self { let mut l=Lexer::new(source); let mut tokens=Vec::new(); while let Some(t)=l.next(){tokens.push(t)} Self{tokens,pos:0,spaces:HashMap::new(),flows:HashMap::new()} }
    fn peek(&self)->Option<&Token>{self.tokens.get(self.pos)}
    fn take(&mut self)->Option<Token>{let t=self.tokens.get(self.pos).cloned(); if t.is_some(){self.pos+=1;} t}
    fn expect(&mut self,t:Token)->Result<(),String>{if self.take()==Some(t){Ok(())}else{Err(format!("unexpected token near {}",self.pos))}}
    fn ident(&mut self)->Result<String,String>{match self.take(){Some(Token::Ident(s))=>Ok(s),x=>Err(format!("expected identifier, got {:?}",x))}}
    pub fn parse(&mut self, registry: &mut PrimitiveRegistry)->Result<Flow,String>{let mut last=None; while self.pos<self.tokens.len(){match self.peek().cloned(){Some(Token::Space)=>self.space_decl()?,Some(Token::Flow)=>{let(n,f)=self.flow_decl(registry)?;self.flows.insert(n,f.clone());last=Some(f)},Some(Token::Ident(_))=>self.primitive_decl(registry)?,x=>return Err(format!("unexpected top-level token {:?}",x))}} last.ok_or_else(||"no flow declaration found".into())}
    fn space_decl(&mut self)->Result<(),String>{self.expect(Token::Space)?;let n=self.ident()?;if self.peek()==Some(&Token::Lt){self.take();while self.peek()!=Some(&Token::Gt){self.take().ok_or("unterminated space parameters")?;}self.expect(Token::Gt)?;}self.expect(Token::Equals)?;let s=self.space_expr()?;self.spaces.insert(n,s);Ok(())}
    fn primitive_decl(&mut self,r:&mut PrimitiveRegistry)->Result<(),String>{let n=self.ident()?;self.expect(Token::Colon)?;let d=self.space_expr()?;self.expect(Token::Arrow)?;let c=self.space_expr()?;r.declare(n,d,c);Ok(())}
    fn flow_decl(&mut self,r:&mut PrimitiveRegistry)->Result<(String,Flow),String>{self.expect(Token::Flow)?;let n=self.ident()?;let declared=if self.peek()==Some(&Token::Colon){self.take();let d=self.space_expr()?;self.expect(Token::Arrow)?;Some((d,self.space_expr()?))}else{None};if self.peek()==Some(&Token::Equals){self.take();let f=self.expr(r)?;self.validate(&f,r)?;if let Some((d,c))=declared{if f.domain()!=d||f.codomain()!=c{return Err(format!("flow {} type annotation mismatch",n));}}Ok((n,f))}else{let(d,c)=declared.ok_or("expected flow body or type annotation")?;r.declare(n.clone(),d.clone(),c.clone());Ok((n.clone(),Flow::Primitive{name:n,domain:d,codomain:c}))}}
    fn space_expr(&mut self)->Result<Space,String>{let n=self.ident()?;if self.peek()==Some(&Token::Lt){self.take();let mut a=Vec::new();loop{match self.take(){Some(Token::Number(v))=>a.push(v),Some(Token::Ident(_))=>{},_=>return Err("invalid generic argument".into())}if self.peek()!=Some(&Token::Comma){break}self.take();}self.expect(Token::Gt)?;let v=*a.first().unwrap_or(&0);return Ok(match n.as_str(){"Tensor"|"Array"=>Space::Tensor{element:"Float32".into(),shape:vec![v]},"Qubit"|"Quantum"=>Space::Quantum{qubits:v},"Organoid"|"MEA"=>Space::Organoid{pins:v},"Logic"=>Space::Logic{bits:v},_=>Space::Scalar(n)})}Ok(self.spaces.get(&n).cloned().unwrap_or(Space::Scalar(n)))}
    fn expr(&mut self,r:&PrimitiveRegistry)->Result<Flow,String>{let mut f=self.parallel(r)?;while self.peek()==Some(&Token::Chain){self.take();f=Flow::Chain(Box::new(f),Box::new(self.parallel(r)?));}Ok(f)}
    fn parallel(&mut self,r:&PrimitiveRegistry)->Result<Flow,String>{let mut f=self.unary(r)?;while self.peek()==Some(&Token::Parallel){self.take();f=Flow::Parallel(Box::new(f),Box::new(self.unary(r)?));}Ok(f)}
    fn unary(&mut self,r:&PrimitiveRegistry)->Result<Flow,String>{if self.peek()==Some(&Token::Feedback){self.take();Ok(Flow::Feedback(Box::new(self.unary(r)?)))}else{self.primary(r)}}
    fn primary(&mut self,r:&PrimitiveRegistry)->Result<Flow,String>{match self.take(){Some(Token::LParen)=>{let f=self.expr(r)?;self.expect(Token::RParen)?;Ok(f)},Some(Token::Grad)=>{self.expect(Token::LParen)?;let f=self.expr(r)?;self.expect(Token::RParen)?;Ok(f.grad(r))},Some(Token::Ident(n))=>self.flows.get(&n).cloned().or_else(||r.get(&n).map(|p|Flow::Primitive{name:n.clone(),domain:p.domain.clone(),codomain:p.codomain.clone()})).ok_or_else(||format!("undefined flow or primitive {}",n)),x=>Err(format!("expected expression, got {:?}",x))}}
    fn validate(&self,f:&Flow,r:&PrimitiveRegistry)->Result<(),String>{match f{Flow::Identity{..}=>Ok(()),Flow::Primitive{name,domain,codomain}=>{let p=r.get(name).ok_or_else(||format!("undefined primitive {}",name))?;if &p.domain!=domain||&p.codomain!=codomain{Err(format!("type mismatch in {}",name))}else{Ok(())}},Flow::Chain(a,b)=>{self.validate(a,r)?;self.validate(b,r)?;if a.codomain()!=b.domain(){Err(format!("chain mismatch {:?} -> {:?}",a.codomain(),b.domain()))}else{Ok(())}},Flow::Parallel(a,b)=>{self.validate(a,r)?;self.validate(b,r)},Flow::Feedback(f)=>self.validate(f,r)}}
}

pub struct C99Emitter;
impl C99Emitter {
    pub fn emit(f:&Flow,r:&PrimitiveRegistry)->String{let mut c=String::from("#include <stdint.h>\n#include <stddef.h>\n#include <string.h>\n\ntypedef struct { int quantum_fd; int mea_fd; } IntelligenceDevices;\n\n");c.push_str("static long long add_i64(long long x){return x+1;}\nstatic long long mul_i64(long long x){return x*2;}\nstatic void tensor_forward(const float in[128],float out[64]){for(size_t i=0;i<64;i++){out[i]=0;for(size_t j=0;j<128;j++)out[i]+=in[j]/128.0f;}}\nstatic void tensor_reverse(const float go[64],float gi[128]){for(size_t j=0;j<128;j++){gi[j]=0;for(size_t i=0;i<64;i++)gi[j]+=go[i]/128.0f;}}\nstatic void quantum_forward(IntelligenceDevices*d,const float*in,float*out){(void)d;memcpy(out,in,64*sizeof(float));}\nstatic void quantum_reverse(IntelligenceDevices*d,const float*go,float*gi){(void)d;memcpy(gi,go,64*sizeof(float));}\nstatic void organoid_forward(IntelligenceDevices*d,const float*in,float*out){(void)d;memcpy(out,in,64*sizeof(float));}\nstatic void organoid_reverse(IntelligenceDevices*d,const float*go,float*gi){(void)d;memcpy(gi,go,64*sizeof(float));}\n\n");c.push_str("void intelligence_forward(long long input,long long*output,IntelligenceDevices*d){long long result=input;(void)d;\n");Self::scalar(f,&mut c,"result");c.push_str("*output=result;}\n\nvoid intelligence_reverse(long long grad_output,long long*grad_input,IntelligenceDevices*d){long long gradient=grad_output;(void)d;\n");Self::scalar(&f.grad(r),&mut c,"gradient");c.push_str("*grad_input=gradient;}\n\nint main(void){IntelligenceDevices d={0,0};long long o=0;intelligence_forward(0,&o,&d);return 0;}\n");c}
    fn scalar(f:&Flow,c:&mut String,v:&str){match f{Flow::Primitive{name,..}=>match name.as_str(){"add"=>c.push_str(&format!("{}=add_i64({});\n",v,v)),"mul"=>c.push_str(&format!("{}=mul_i64({});\n",v,v)),n=>c.push_str(&format!("/* {} uses a typed device backend */\n",n)),},Flow::Identity{..}=>{},Flow::Chain(a,b)=>{Self::scalar(a,c,v);Self::scalar(b,c,v)},Flow::Parallel(a,b)=>{Self::scalar(a,c,v);Self::scalar(b,c,v)},Flow::Feedback(x)=>Self::scalar(x,c,v)}}
}

fn main(){let a:Vec<String>=env::args().collect();if a.len()<2{eprintln!("usage: intelligence <program.cat>");return}let s=match fs::read_to_string(&a[1]){Ok(v)=>v,Err(e)=>{eprintln!("read error: {}",e);return}};let mut r=PrimitiveRegistry::new();let mut p=Parser::new(&s);let f=match p.parse(&mut r){Ok(v)=>v,Err(e)=>{eprintln!("parse/type error: {}",e);return}};if let Err(e)=fs::write("payload.c",C99Emitter::emit(&f,&r)){eprintln!("write error: {}",e);return}match Command::new("clang").args(["-std=c99","-O3","payload.c","-o","binary_app"]).status(){Ok(s)if s.success()=>println!("Compiled successfully!"),Ok(_)=>eprintln!("C compilation failed"),Err(e)=>eprintln!("clang error: {}",e)}}
