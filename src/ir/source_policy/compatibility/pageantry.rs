//! Loader-owned pageantry collection and proc-macro recognition.
mod collection;
mod shim;

pub(crate) use collection::collect;
pub use collection::scan_rust_source;
