//! Canonical syntax carriers and memoized macro dialect projections.
mod source;
pub(crate) use source::DatabaseSource;
#[cfg(any(
    feature = "panics",
    feature = "antipatterns",
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
    feature = "antipatterns",
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
#[cfg(feature = "antipatterns")]
pub(crate) use tokens::render_excluding as token_text_excluding;
#[cfg(any(
    feature = "tracing",
    feature = "internal_error_chain",
    feature = "panics",
    feature = "antipatterns"
))]
mod macro_syntax;
#[cfg(any(
    feature = "tracing",
    feature = "internal_error_chain",
    feature = "panics",
    feature = "antipatterns"
))]
pub(crate) use macro_syntax::DatabaseMacroSyntax;
#[cfg(feature = "internal_error_chain")]
mod cfg_syntax;
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

#[cfg(feature = "antipatterns")]
mod antipatterns;
