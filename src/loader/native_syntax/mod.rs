//! Canonical syntax carriers and memoized macro dialect projections.
mod source;
pub(crate) use source::DatabaseSource;
#[cfg(any(feature = "tracing", feature = "internal_error_chain"))]
mod macro_syntax;
#[cfg(any(feature = "tracing", feature = "internal_error_chain"))]
pub(crate) use macro_syntax::DatabaseMacroSyntax;
#[cfg(feature = "internal_error_chain")]
mod meta_syntax;
#[cfg(any(feature = "cli_layout", feature = "internal_error_chain"))]
mod modules;
#[cfg(any(feature = "cli_layout", feature = "internal_error_chain"))]
pub(crate) use modules::module_path;
#[cfg(feature = "verus_ir")]
mod verus_syntax;
#[cfg(all(feature = "verus_ir", feature = "visibility"))]
pub(crate) use verus_syntax::DatabaseVerusSyntax;
