use crate::backend::{Backend, BackendKind};
use crate::language::{Effect, PrimitiveFamily, Signature, Space, TypedFlow};

pub struct C99Backend;
impl C99Backend { pub fn new()->Self{Self} pub fn preamble(&self)->String{"#include <stddef.h>\n#include <stdint.h>\n#include <string.h>\n\n".into()} pub fn supports_signature(&self,s:&Signature)->Result<(),String>{if s.effects.contains(&Effect::Nondeterministic){Err("portable C99 backend rejects nondeterministic effects".into())}else{Ok(())}} }
impl Backend for C99Backend {
    fn kind(&self)->BackendKind{BackendKind::PortableC99}
    fn supports(&self,space:&Space)->bool{match space{Space::Unit|Space::Scalar(_)|Space::Tensor{..}|Space::Quantum{..}|Space::Organoid{..}|Space::Logic{..}|Space::Product(_)=>true}}
    fn lower(&self,flow:&TypedFlow)->Result<String,String>{let mut out=String::new();emit(flow,&mut out);Ok(out)}
}
fn emit(flow:&TypedFlow,out:&mut String){match flow{TypedFlow::Primitive{name,signature}=>match (&signature.domain,&signature.codomain){(Space::Scalar(_),Space::Scalar(_))=>out.push_str(&format!("/* scalar primitive {name} */\n")),(Space::Tensor{shape:a,..},Space::Tensor{shape:b,..})=>out.push_str(&format!("/* tensor primitive {name}: {} -> {} */\n",a.iter().product::<usize>(),b.iter().product::<usize>())),(Space::Quantum{qubits:a},Space::Quantum{qubits:b})=>out.push_str(&format!("/* quantum primitive {name}: {a} -> {b} qubits */\n")),(Space::Organoid{pins:a},Space::Organoid{pins:b})=>out.push_str(&format!("/* organoid primitive {name}: {a} -> {b} channels */\n")),(Space::Logic{bits:a},Space::Logic{bits:b})=>out.push_str(&format!("/* logic primitive {name}: {a} -> {b} bits */\n")),_=>out.push_str(&format!("/* product primitive {name} */\n"))},TypedFlow::Identity(_)=>{},TypedFlow::Chain(a,b)|TypedFlow::Parallel(a,b)=>{emit(a,out);emit(b,out)},TypedFlow::Feedback{inner,..}|TypedFlow::Gradient{inner,..}=>emit(inner,out)}}
}
