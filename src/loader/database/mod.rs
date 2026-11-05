//! Canonical source inputs, tracked queries, and shared IR projection.

#[cfg(any(feature = "tracing", feature = "internal_error_chain"))]
mod macro_syntax;
#[cfg(any(feature = "cli_layout", feature = "internal_error_chain"))]
mod modules;
#[cfg(any(feature = "cli_layout", feature = "internal_error_chain"))]
pub(crate) use modules::module_path;
#[cfg(feature = "internal_error_chain")]
mod meta_syntax;
mod options;
mod populate;
mod queries;
mod recording;
mod session;
mod source;
mod types;

#[cfg(any(feature = "tracing", feature = "internal_error_chain"))]
pub(crate) use macro_syntax::DatabaseMacroSyntax;
pub use options::SemanticCargoOptions;
pub use session::SourceDatabase;
pub(crate) use source::DatabaseSource;
