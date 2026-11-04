//! Canonical source inputs, tracked queries, and shared IR projection.

#[cfg(feature = "tracing")]
mod macro_syntax;
mod populate;
mod queries;
mod recording;
mod session;
mod source;
mod types;

#[cfg(feature = "tracing")]
pub(crate) use macro_syntax::DatabaseMacroSyntax;
pub use session::SourceDatabase;
pub(crate) use source::DatabaseSource;
