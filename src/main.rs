use std::{collections::HashMap, env, fs, process::Command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Space { Scalar(String), Tensor(usize), Qubit(usize), Organoid(usize), Logic(usize), Product(Vec<Space>), Unit }

impl Space {
    fn product(a: Space, b: Space) -> Space {
        let mut out = match a { Space::Product(v) => v, x => vec![x] };
        match b { Space::Product(v) => out.extend(v), x => out.push(x) }
        Space::Product(out)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimitiveFamily { Scalar, Tensor, Quantum, Organoid, Logic }

#[derive(Debug, Clone)]
pub struct Primitive { pub name: String, pub dom: Space, pub cod: Space, pub family: PrimitiveFamily, pub forward_c: String, pub adjoint_c: String }

#[derive(Debug, Clone)]
pub struct PrimitiveRegistry { pub primitives: HashMap<String, Primitive>, pub spaces: HashMap<String, Space> }

impl PrimitiveRegistry {
    fn new() -> Self {
        let mut r = Self { primitives: HashMap::new(), spaces: HashMap::new() };
        r.register(Primitive { name: "add".into(), dom: Space::Scalar("f64".into()), cod: Space::Scalar("f64".into()), family: PrimitiveFamily::Scalar, forward_c: "*out = *in + 1.0;".into(), adjoint_c: "*grad_in = *grad_out;".into() });
        r.register(Primitive { name: "mul".into(), dom: Space::Scalar("f64".into()), cod: Space::Scalar("f64".into()), family: PrimitiveFamily::Scalar, forward_c: "*out = *in * 2.0;".into(), adjoint_c: "*grad_in = *grad_out * 2.0;".into() });
        r.register(Primitive { name: "neural_layer".into(), dom: Space::Tensor(128), cod: Space::Tensor(64), family: PrimitiveFamily::Tensor, forward_c: "tensor_forward(in, out, 128, 64);".into(), adjoint_c: "tensor_reverse(grad_out, grad_in, 128, 64);".into() });
        r.register(Primitive { name: "quantum_gate".into(), dom: Space::Qubit(2), cod: Space::Qubit(2), family: PrimitiveFamily::Quantum, forward_c: "quantum_forward(dev, in, out, 2);".into(), adjoint_c: "quantum_reverse(dev, grad_out, grad_in, 2);".into() });
        for name in ["organoid_step", "organoid_pulse"] { r.register(Primitive { name: name.into(), dom: Space::Organoid(64), cod: Space::Organoid(64), family: PrimitiveFamily::Organoid, forward_c: "organoid_forward(dev, in, out, 64);".into(), adjoint_c: "organoid_reverse(dev, grad_out, grad_in, 64);".into() }); }
        r.register(Primitive { name: "logic_step".into(), dom: Space::Logic(8), cod: Space::Logic(8), family: PrimitiveFamily::Logic, forward_c: "logic_forward(in, out, 8);".into(), adjoint_c: "logic_reverse(grad_out, grad_in, 8);".into() });
        r
    }
    fn register(&mut self, p: Primitive) { self.primitives.insert(p.name.clone(), p); }
    fn get(&self, n: &str) -> Option<&Primitive> { self.primitives.get(n) }
    fn declare_primitive(&mut self, name: String, dom: Space, cod: Space) {
        let family = family_for(&dom, &cod);
        self.register(Primitive { name: name.clone(), dom, cod, family, forward_c: format!("/* {} forward */", name), adjoint_c: format!("/* {} adjoint */", name) });
    }
}

fn family_for(a: &Space, b: &Space) -> PrimitiveFamily { match (a,b) { (Space::Tensor(_),_)|(_,Space::Tensor(_)) => PrimitiveFamily::Tensor, (Space::Qubit(_),_)|(_,Space::Qubit(_)) => PrimitiveFamily::Quantum, (Space::Organoid(_),_)|(_,Space::Organoid(_)) => PrimitiveFamily::Organoid, (Space::Logic(_),_)|(_,Space::Logic(_)) => PrimitiveFamily::Logic, _ => PrimitiveFamily::Scalar } }

#[derive(Debug, Clone)]
pub enum Flow { Primitive(String), Chain(Box<Flow>,Box<Flow>), Parallel(Box<Flow>,Box<Flow>), Feedback(Box<Flow>), Identity(Space), Derivative(Box<Flow>) }

impl Flow {
    fn ty(&self, r: &PrimitiveRegistry, flows: &HashMap<String, Flow>) -> Result<(Space,Space),String> {
        match self {
            Flow::Primitive(n) => r.get(n).map(|p|(p.dom.clone(),p.cod.clone())).or_else(|| flows.get(n).and_then(|f|f.ty(r,flows).ok())).ok_or_else(||format!("undefined flow or primitive `{n}`")),
            Flow::Identity(s) => Ok((s.clone(),s.clone())),
            Flow::Chain(a,b) => { let (ad,ac)=a.ty(r,flows)?; let (_,bd)=b.ty(r,flows)?; if ac!=bd { return Err(format!("chain mismatch: {:?} != {:?}",ac,bd)); } Ok((ad,b.ty(r,flows)?.1)) }
            Flow::Parallel(a,b) => { let (ad,ac)=a.ty(r,flows)?; let (bd,bc)=b.ty(r,flows)?; Ok((Space::product(ad,bd),Space::product(ac,bc))) }
            Flow::Feedback(f) => { let (d,c)=f.ty(r,flows)?; match (d,c) { (Space::Product(mut di),Space::Product(mut co)) if di.len()>=2 && co.len()>=2 => { let u_in=di.pop().unwrap(); let u_out=co.pop().unwrap(); if u_in!=u_out { return Err("trace requires equal feedback spaces".into()); } Ok((Space::Product(di),Space::Product(co))) }, _ => Err("trace requires f : A || U -> B || U".into()) } }
            Flow::Derivative(f) => { let (d,c)=f.ty(r,flows)?; Ok((Space::product(d,c.clone()),d)) }
        }
    }
    fn grad(&self) -> Flow { match self { Flow::Primitive(n)=>Flow::Primitive(format!("grad_{n}")), Flow::Chain(a,b)=>Flow::Chain(Box::new(b.grad()),Box::new(a.grad())), Flow::Parallel(a,b)=>Flow::Parallel(Box::new(a.grad()),Box::new(b.grad())), Flow::Feedback(f)=>Flow::Feedback(Box::new(f.grad())), Flow::Identity(s)=>Flow::Identity(s.clone()), Flow::Derivative(f)=>f.grad() } }
}

#[derive(Debug, Clone, PartialEq)]
enum Token { Space, Flow, Grad, Id, Ident(String), Num(usize), Eq, Colon, Arrow, Chain, Parallel, Tilde, LParen, RParen, LAngle, RAngle, Comma, Eof }

struct Lexer<'a> { src: &'a [u8], pos: usize }
impl<'a> Lexer<'a> {
    fn new(s:&'a str)->Self{Self{src:s.as_bytes(),pos:0}}
    fn tokens(&mut self)->Vec<Token>{ let mut v=Vec::new(); while let Some(t)=self.next(){v.push(t)} v.push(Token::Eof); v }
    fn next(&mut self)->Option<Token>{
        while self.pos<self.src.len() && self.src[self.pos].is_ascii_whitespace(){self.pos+=1}
        if self.pos>=self.src.len(){return None} if self.src[self.pos..].starts_with(b"//"){while self.pos<self.src.len()&&self.src[self.pos]!=b'\n'{self.pos+=1}return self.next()}
        for (s,t) in [(b">>".as_slice(),Token::Chain),(b"||".as_slice(),Token::Parallel),(b"->".as_slice(),Token::Arrow)]{if self.src[self.pos..].starts_with(s){self.pos+=2;return Some(t)}}
        let c=self.src[self.pos]; self.pos+=1; Some(match c { b'='=>Token::Eq,b':'=>Token::Colon,b'~'=>Token::Tilde,b'('=>Token::LParen,b')'=>Token::RParen,b'<'=>Token::LAngle,b'>'=>Token::RAngle,b','= '=>Token::Comma,_ if c.is_ascii_digit()=>{let mut n=(c-b'0')as usize;while self.pos<self.src.len()&&self.src[self.pos].is_ascii_digit(){n=n*10+(self.src[self.pos]-b'0')as usize;self.pos+=1}Token::Num(n)}, _ if c.is_ascii_alphabetic()||c==b'_' => {let start=self.pos-1;while self.pos<self.src.len()&&(self.src[self.pos].is_ascii_alphanumeric()||self.src[self.pos]==b'_'){self.pos+=1}match std::str::from_utf8(&self.src[start..self.pos]).unwrap(){"space"=>Token::Space,"flow"=>Token::Flow,"grad"=>Token::Grad,"id"=>Token::Id,s=>Token::Ident(s.into())}}, _=>return self.next()})
    }
}

pub struct FlowDecl { name:String, dom:Option<Space>, cod:Option<Space>, body:Flow }
pub struct Parser { ts:Vec<Token>, pos:usize, flows:HashMap<String,Flow> }
impl Parser {
    fn new(ts:Vec<Token>)->Self{Self{ts,pos:0,flows:HashMap::new()}}
    fn peek(&self)->&Token{&self.ts[self.pos]}
    fn take(&mut self)->Token{let t=self.ts[self.pos].clone();if self.pos+1<self.ts.len(){self.pos+=1}t}
    fn expect(&mut self,t:Token)->Result<(),String>{if self.take()==t{Ok(())}else{Err(format!("expected {:?} near token {}",t,self.pos))}}
    fn ident(&mut self)->Result<String,String>{match self.take(){Token::Ident(s)=>Ok(s),x=>Err(format!("expected identifier, got {:?}",x))}}
    fn parse(&mut self,r:&mut PrimitiveRegistry)->Result<Vec<FlowDecl>,String>{let mut out=Vec::new();while *self.peek()!=Token::Eof{match self.peek(){Token::Space=>self.space(r)?,Token::Flow=>{let d=self.flow(r)?;self.flows.insert(d.name.clone(),d.body.clone());out.push(d)},Token::Ident(_)=>self.primitive(r)?,x=>return Err(format!("unexpected top-level token {:?}",x))}}Ok(out)}
    fn space(&mut self,r:&mut PrimitiveRegistry)->Result<(),String>{self.take();let n=self.ident()?;self.expect(Token::Eq)?;let s=self.space_spec(r)?;r.spaces.insert(n,s);Ok(())}
    fn primitive(&mut self,r:&mut PrimitiveRegistry)->Result<(),String>{let n=self.ident()?;self.expect(Token::Colon)?;let d=self.space_spec(r)?;self.expect(Token::Arrow)?;let c=self.space_spec(r)?;r.declare_primitive(n,d,c);Ok(())}
    fn space_spec(&mut self,r:&PrimitiveRegistry)->Result<Space,String>{match self.take(){Token::Ident(n)=>{if let Some(s)=r.spaces.get(&n){return Ok(s.clone())} if self.peek()==&Token::LAngle{self.take();let k=match self.take(){Token::Num(n)=>n,_=>return Err("expected dimension".into())};self.expect(Token::RAngle)?;return Ok(match n.as_str(){"Tensor"|"Array"=>Space::Tensor(k),"Qubit"|"Quantum"=>Space::Qubit(k),"Organoid"|"MEA"=>Space::Organoid(k),"Logic"|"Bits"=>Space::Logic(k),_=>Space::Scalar(n)})}Ok(Space::Scalar(n))},Token::LParen=>{let mut parts=vec![self.space_spec(r)?];while self.peek()==&Token::Parallel{self.take();parts.push(self.space_spec(r)?)}self.expect(Token::RParen)?;Ok(Space::Product(parts))},x=>Err(format!("expected space specification, got {:?}",x))}}
    fn flow(&mut self,r:&mut PrimitiveRegistry)->Result<FlowDecl,String>{self.take();let name=self.ident()?;let(mut d,mut c)=(None,None);if self.peek()==&Token::Colon{self.take();d=Some(self.space_spec(r)?);self.expect(Token::Arrow)?;c=Some(self.space_spec(r)?)}self.expect(Token::Eq)?;let body=self.expr(r)?;Ok(FlowDecl{name,dom:d,cod:c,body})}
    fn expr(&mut self,r:&PrimitiveRegistry)->Result<Flow,String>{let mut x=self.parallel(r)?;while self.peek()==&Token::Chain{self.take();x=Flow::Chain(Box::new(x),Box::new(self.parallel(r)?))}Ok(x)}
    fn parallel(&mut self,r:&PrimitiveRegistry)->Result<Flow,String>{let mut x=self.primary(r)?;while self.peek()==&Token::Parallel{self.take();x=Flow::Parallel(Box::new(x),Box::new(self.primary(r)?))}Ok(x)}
    fn primary(&mut self,r:&PrimitiveRegistry)->Result<Flow,String>{match self.take(){Token::Tilde=>Ok(Flow::Feedback(Box::new(self.primary(r)?))),Token::Grad=>{self.expect(Token::LParen)?;let f=self.expr(r)?;self.expect(Token::RParen)?;Ok(Flow::Derivative(Box::new(f)))},Token::Id=>{if self.peek()==&Token::LParen{self.take();let s=self.space_spec(r)?;self.expect(Token::RParen)?;Ok(Flow::Identity(s))}else{Ok(Flow::Identity(Space::Unit))}},Token::Ident(n)=>Ok(self.flows.get(&n).cloned().unwrap_or(Flow::Primitive(n))),Token::LParen=>{let x=self.expr(r)?;self.expect(Token::RParen)?;Ok(x)},x=>Err(format!("unexpected flow token {:?}",x))}}
}

struct Emitter<'a>{r:&'a PrimitiveRegistry}
impl<'a> Emitter<'a>{fn emit(&self,decls:&[FlowDecl])->String{let mut c=String::from("#include <stdint.h>\n#include <stddef.h>\n#include <stdio.h>\n#include <string.h>\ntypedef struct { int qasm_fd; int mea_fd; } IntelligenceDevices;\nstatic void tensor_forward(const float*i,float*o,int n,int m){for(int x=0;x<m;x++){o[x]=0;for(int y=0;y<n;y++)o[x]+=i[y]*0.01f;}}\nstatic void tensor_reverse(const float*o,float*i,int n,int m){for(int x=0;x<n;x++){i[x]=0;for(int y=0;y<m;y++)i[x]+=o[y]*0.01f;}}\nstatic void quantum_forward(IntelligenceDevices*d,const float*i,float*o,int q){(void)d;memcpy(o,i,(1<<q)*2*sizeof(float));}\nstatic void quantum_reverse(IntelligenceDevices*d,const float*o,float*i,int q){(void)d;memcpy(i,o,(1<<q)*2*sizeof(float));}\nstatic void organoid_forward(IntelligenceDevices*d,const float*i,float*o,int n){(void)d;memcpy(o,i,n*sizeof(float));}\nstatic void organoid_reverse(IntelligenceDevices*d,const float*o,float*i,int n){(void)d;memcpy(i,o,n*sizeof(float));}\nstatic void logic_forward(const uint8_t*i,uint8_t*o,int n){memcpy(o,i,(n+7)/8);}\nstatic void logic_reverse(const uint8_t*o,uint8_t*i,int n){memcpy(i,o,(n+7)/8);}\n\n");for(d in decls{c.push_str(&format!("/* flow {} */\nvoid {}_forward(IntelligenceDevices*dev,const void*in,void*out){{(void)dev;(void)in;(void)out;}}\n",d.name,d.name))}c.push_str("int main(void){IntelligenceDevices dev={0,0};(void)dev;puts(\"Intelligence categorical runtime ready\");return 0;}\n");c} }

fn main(){let args:Vec<String>=env::args().collect();let path=args.get(1).map(String::as_str).unwrap_or("program.cat");let src=match fs::read_to_string(path){Ok(s)=>s,Err(e)=>{eprintln!("read error: {e}");return}};let mut r=PrimitiveRegistry::new();let mut l=Lexer::new(&src);let mut p=Parser::new(l.tokens());let decls=match p.parse(&mut r){Ok(v)=>v,Err(e)=>{eprintln!("parse error: {e}");return}};for d in &decls{let ty=match d.body.ty(&r,&p.flows){Ok(t)=>t,Err(e)=>{eprintln!("type error in {}: {e}",d.name);return}};if let(Some(a),Some(b))=(&d.dom,&d.cod){if ty!=(a.clone(),b.clone()){eprintln!("annotation mismatch in {}",d.name);return}}}let code=Emitter{r:&r}.emit(&decls);if let Err(e)=fs::write("payload.c",code){eprintln!("write error: {e}");return}match Command::new("clang").args(["-std=c99","-O2","payload.c","-o","binary_app"]).status(){Ok(s)if s.success()=>println!("Compiled successfully: binary_app"),Ok(_)=>eprintln!("C compilation failed"),Err(e)=>eprintln!("clang error: {e}")}}
