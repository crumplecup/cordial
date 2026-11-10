//! Helpers for reading names and bodies from registered contract fragments.

use super::super::token_tree::{Delimiter, parse, render};
use tracing::instrument;

/// Find the identifier immediately following a top-level `fn` token in
/// `fragment`'s own source text, without requiring the text to parse as
/// a complete, valid Rust item.
#[instrument(level = "debug")]
pub(super) fn fragment_fn_name(fragment: &str) -> Option<String> {
    let tokens = parse(fragment)?;
    tokens.windows(2).find_map(|pair| {
        (pair[0].ident() == Some("fn"))
            .then(|| pair[1].ident().map(str::to_string))
            .flatten()
    })
}

/// Find the brace-delimited body immediately following the top-level
/// `fn <name>` pair in `fragment`'s own source text, normalized the same
/// way a clause's own tokens are.
#[instrument(level = "debug")]
pub(super) fn fragment_fn_body_text(fragment: &str) -> Option<String> {
    let tokens = parse(fragment)?;
    let fn_idx = tokens
        .windows(2)
        .position(|pair| pair[0].ident() == Some("fn"))?;
    tokens[fn_idx..]
        .iter()
        .find_map(|token| token.group(Delimiter::Brace).map(render))
}
