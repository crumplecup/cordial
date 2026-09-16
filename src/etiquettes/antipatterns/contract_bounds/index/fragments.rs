//! Helpers for reading names and bodies from registered contract fragments.

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use tracing::instrument;

use super::tokens::normalize_tokens;

/// Find the identifier immediately following a top-level `fn` token in
/// `fragment`'s own source text, without requiring the text to parse as
/// a complete, valid Rust item.
#[instrument(level = "debug")]
pub(super) fn fragment_fn_name(fragment: &str) -> Option<String> {
    let tokens: TokenStream = fragment.parse().ok()?;
    let items: Vec<TokenTree> = tokens.into_iter().collect();
    items.windows(2).find_map(|pair| {
        let (TokenTree::Ident(keyword), TokenTree::Ident(name)) = (&pair[0], &pair[1]) else {
            return None;
        };
        (keyword == "fn").then(|| name.to_string())
    })
}

/// Find the brace-delimited body immediately following the top-level
/// `fn <name>` pair in `fragment`'s own source text, normalized the same
/// way a clause's own tokens are.
#[instrument(level = "debug")]
pub(super) fn fragment_fn_body_text(fragment: &str) -> Option<String> {
    let tokens: TokenStream = fragment.parse().ok()?;
    let items: Vec<TokenTree> = tokens.into_iter().collect();
    let fn_idx = items
        .windows(2)
        .position(|pair| matches!(&pair[0], TokenTree::Ident(keyword) if keyword == "fn"))?;
    items[fn_idx..].iter().find_map(|tt| match tt {
        TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
            Some(normalize_tokens(group.stream()))
        }
        _ => None,
    })
}
