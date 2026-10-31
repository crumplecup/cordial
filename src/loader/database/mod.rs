//! Canonical source inputs, tracked queries, and shared IR projection.

mod populate;
mod queries;
mod recording;
mod session;
mod source;
mod types;

pub use session::SourceDatabase;
pub(crate) use source::DatabaseSource;
