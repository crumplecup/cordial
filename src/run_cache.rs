//! Results of a run, reused while the inputs that shaped them are unchanged.
//!
//! The expensive part of a run is analysing the project: loading the workspace
//! into rust-analyzer and resolving every function. Its results are plain data
//! in the IR, so they are kept on disk under a digest of everything that could
//! change them. A later run with the same digest reads them back and never loads
//! the workspace; a run whose inputs changed analyses again and replaces the
//! entry. Nothing is reused across different inputs, so nothing can be stale.

mod key;
mod store;

pub(crate) use key::{KeyRequest, gather};
pub(crate) use store::RunCache;
