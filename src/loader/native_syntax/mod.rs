//! Canonical syntax carriers and memoized macro dialect projections.
mod source;
pub(crate) use source::DatabaseSource;
#[cfg(any(
    feature = "panics",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "visibility",
    feature = "cli_layout",
))]
mod tokens;
#[cfg(any(
    feature = "panics",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "visibility",
    feature = "cli_layout",
))]
pub(crate) use tokens::render as token_text;
#[cfg(any(
    feature = "tracing",
    feature = "internal_error_chain",
    feature = "panics"
))]
mod macro_syntax;
#[cfg(any(
    feature = "tracing",
    feature = "internal_error_chain",
    feature = "panics"
))]
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
