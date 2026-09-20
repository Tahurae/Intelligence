use crate::language::{AstFlow, Derivative, Effect, FlowDecl, Signature, Space};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ParsedProgram { pub flows: Vec<FlowDecl> }

#[derive(Debug, Clone, PartialEq)]
enum Token { Ident(String), Number(usize), Space, Flow, Grad, Id, Equals, Colon, Arrow, Chain, Parallel, Tilde, LParen, RParen, LAngle, RAngle, Eof }

fn lex(source: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = source.chars().collect(); let mut i = 0; let mut out = Vec::new();
    while i < chars.len() { if chars[i].is_whitespace() { i += 1; continue; } if chars[i] == '/' && chars.get(i+1) == Some(&'/') { i += 2; while i < chars.len() && chars[i] != '\n' { i += 1; } continue; }
        if i + 1 < chars.len() { let pair: String = chars[i..i+2].iter().collect(); let token = match pair.as_str() { ">>"=>Some(Token::Chain), "||"=>Some(Token::Parallel), "->"=>Some(Token::Arrow), _=>None }; if let Some(t)=token { out.push(t); i+=2; continue; } }
        match chars[i] { '='=>out.push(Token::Equals), ':'=>out.push(Token::Colon), '~'=>out.push(Token::Tilde), '('=>out.push(Token::LParen), ')'=>out.push(Token::RParen), '<'=>out.push(Token::LAngle), '>'=>out.push(Token::RAngle), c if c.is_ascii_digit()=>{let mut n=0;while i<chars.len()&&chars[i].is_ascii_digit(){n=n*10+(chars[i] as usize-'0' as usize);i+=1;}out.push(Token::Number(n));continue}, c if c.is_ascii_alphabetic()||c=='_'=>{let start=i;while i<chars.len()&&(chars[i].is_ascii_alphanumeric()||chars[i]=='_'){i+=1}let word:String=chars[start..i].iter().collect();out.push(match word.as_str(){"space"=>Token::Space,"flow"=>Token::Flow,"grad"=>Token::Grad,"id"=>Token::Id,_=>Token::Ident(word)});continue}, c=>return Err(format!("unexpected character `{c}`")) } i+=1; }
    out.push(Token::Eof); Ok(out)
}

pub fn parse(source: &str, primitives: &mut HashMap<String, Signature>) -> Result<ParsedProgram, String> {
    let mut p = Parser { tokens: lex(source)?, pos: 0, spaces: HashMap::new() }; let mut flows = Vec::new();
    while !matches!(p.peek(), Token::Eof) { match p.peek() { Token::Space=>p.space()?, Token::Ident(_)=>p.primitive(primitives)?, Token::Flow=>flows.push(p.flow()?), t=>return Err(format!("unexpected top-level token {t:?}")) } }
    Ok(ParsedProgram { flows })
}
struct Parser { tokens: Vec<Token>, pos: usize, spaces: HashMap<String, Space> }
impl Parser {
    fn peek(&self)->&Token{&self.tokens[self.pos]} fn take(&mut self)->Token{let t=self.tokens[self.pos].clone();self.pos+=1;t} fn expect(&mut self,w:Token)->Result<(),String>{let a=self.take();if a==w{Ok(())}else{Err(format!("expected {w:?}, got {a:?}"))}} fn ident(&mut self)->Result<String,String>{match self.take(){Token::Ident(s)=>Ok(s),x=>Err(format!("expected identifier, got {x:?}"))}}
    fn space(&mut self)->Result<(),String>{self.take();let n=self.ident()?;self.expect(Token::Equals)?;let s=self.space_spec()?;if self.spaces.insert(n.clone(),s).is_some(){return Err(format!("space `{n}` already declared"))}Ok(())}
    fn primitive(&mut self,primitives:&mut HashMap<String,Signature>)->Result<(),String>{let n=self.ident()?;self.expect(Token::Colon)?;let d=self.space_spec()?;self.expect(Token::Arrow)?;let c=self.space_spec()?;if primitives.contains_key(&n){return Err(format!("primitive `{n}` already declared"))}primitives.insert(n,Signature{domain:d,codomain:c,effects:vec![Effect::Pure],derivative:Derivative::ReverseMode});Ok(())}
    fn space_spec(&mut self)->Result<Space,String>{match self.take(){Token::Ident(n)=>{if let Some(s)=self.spaces.get(&n){return Ok(s.clone())}let dim=if matches!(self.peek(),Token::LAngle){self.take();let n=match self.take(){Token::Number(n)=>n,x=>return Err(format!("expected dimension, got {x:?}"))};self.expect(Token::RAngle)?;Some(n)}else{None};match(n.as_str(),dim){("Tensor"|"Array",Some(n))=>Ok(Space::Tensor{element:"f32".into(),shape:vec![n]}),("Quantum"|"Qubit",Some(n))=>Ok(Space::Quantum{qubits:n}),("Organoid"|"MEA",Some(n))=>Ok(Space::Organoid{pins:n}),("Logic"|"Bits",Some(n))=>Ok(Space::Logic{bits:n}),(name,None) if ["Tensor","Array","Quantum","Qubit","Organoid","MEA","Logic","Bits"].contains(&name)=>Err(format!("{name} requires <dimension>")),(name,Some(_))=>Err(format!("unknown parameterized space `{name}`")),(name,None)=>Err(format!("undeclared space `{name}`"))}},Token::LParen=>{let mut v=vec![self.space_spec()?];while matches!(self.peek(),Token::Parallel){self.take();v.push(self.space_spec()?)}self.expect(Token::RParen)?;if v.len()<2{Err("product requires at least two spaces".into())}else{Ok(Space::Product(v))}},x=>Err(format!("expected space, got {x:?}"))}}
    fn flow(&mut self)->Result<FlowDecl,String>{self.take();let name=self.ident()?;let sig=if matches!(self.peek(),Token::Colon){self.take();let d=self.space_spec()?;self.expect(Token::Arrow)?;let c=self.space_spec()?;Some((d,c))}else{None};self.expect(Token::Equals)?;Ok(FlowDecl{name,signature:sig,body:self.expr()?})}
    fn expr(&mut self)->Result<AstFlow,String>{let mut f=self.parallel()?;while matches!(self.peek(),Token::Chain){self.take();f=AstFlow::Chain(Box::new(f),Box::new(self.parallel()?))}Ok(f)}
    fn parallel(&mut self)->Result<AstFlow,String>{let mut f=self.primary()?;while matches!(self.peek(),Token::Parallel){self.take();f=AstFlow::Parallel(Box::new(f),Box::new(self.primary()?))}Ok(f)}
    fn primary(&mut self)->Result<AstFlow,String>{match self.take(){Token::Ident(n)=>Ok(AstFlow::Name(n)),Token::Id=>{if matches!(self.peek(),Token::LParen){self.take();let s=self.space_spec()?;self.expect(Token::RParen)?;Ok(AstFlow::Identity(s))}else{Ok(AstFlow::Identity(Space::Unit))}},Token::Tilde=>Ok(AstFlow::Feedback(Box::new(self.primary()?))),Token::Grad=>{self.expect(Token::LParen)?;let f=self.expr()?;self.expect(Token::RParen)?;Ok(AstFlow::Gradient(Box::new(f)))},Token::LParen=>{let f=self.expr()?;self.expect(Token::RParen)?;Ok(f)},x=>Err(format!("unexpected flow token {x:?}"))}}
}
