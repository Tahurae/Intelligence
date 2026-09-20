//! Backend contract. The core language depends on this trait, never on a
//! particular operating system, runtime, accelerator, or device.

use crate::language::{Space, TypedFlow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind { PortableC99 }

pub trait Backend {
    fn kind(&self) -> BackendKind;
    fn supports(&self, space: &Space) -> bool;
    fn lower(&self, flow: &TypedFlow) -> Result<String, String>;
}
