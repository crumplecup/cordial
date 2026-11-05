//! Public graph interfaces and their concrete storage implementation.

mod evidence;
mod read;
mod storage;
mod write;

pub use read::IrView;
pub use write::IrMut;
