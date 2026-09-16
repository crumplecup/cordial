//! Token normalization and top-level token-shape utilities.

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use tracing::instrument;

/// Drop the `::` a call-site turbofish (`Type::<Args>`) writes before its
/// generic argument list.
#[instrument(level = "debug")]
pub(super) fn strip_turbofish(text: &str) -> String {
    text.replace(" :: <", " <")
}

/// Strip every top-level `#[trigger]` attribute from `tokens`.
#[instrument(level = "debug", skip(tokens))]
pub(super) fn strip_trigger_attrs(tokens: TokenStream) -> TokenStream {
    let items: Vec<TokenTree> = tokens.into_iter().collect();
    let mut out = Vec::with_capacity(items.len());
    let mut i = 0;
    while i < items.len() {
        if let (TokenTree::Punct(hash), Some(TokenTree::Group(group))) =
            (&items[i], items.get(i + 1))
            && hash.as_char() == '#'
            && group.delimiter() == Delimiter::Bracket
            && group.stream().to_string() == "trigger"
        {
            i += 2;
            continue;
        }
        out.push(items[i].clone());
        i += 1;
    }
    out.into_iter().collect()
}

/// Recognize a whole-clause bare call `name(...)` or `!name(...)`
/// directly from tokens without requiring the argument list to parse as
/// plain Rust syntax.
#[instrument(level = "debug", skip(clause))]
pub(super) fn bare_named_call_name(clause: TokenStream) -> Option<String> {
    let items: Vec<TokenTree> = clause.into_iter().collect();
    let items = match items.as_slice() {
        [TokenTree::Punct(punct), rest @ ..] if punct.as_char() == '!' => rest,
        rest => rest,
    };

    match items {
        [TokenTree::Ident(name), TokenTree::Group(group)]
            if group.delimiter() == Delimiter::Parenthesis =>
        {
            Some(name.to_string())
        }
        _ => None,
    }
}

/// [`bare_named_call_name`], extended to also recognize a Creusot/Verus
/// bare call carrying a leading outer attribute (`#[trigger] name(...)`).
#[instrument(level = "debug", skip(clause))]
pub(super) fn named_call_name_allowing_leading_attr(clause: TokenStream) -> Option<String> {
    if let Some(name) = bare_named_call_name(clause.clone()) {
        return Some(name);
    }
    let expr = syn::parse2::<syn::Expr>(clause).ok()?;
    let call = match &expr {
        syn::Expr::Call(call) => call,
        syn::Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Not(_)) => {
            match unary.expr.as_ref() {
                syn::Expr::Call(call) => call,
                _ => return None,
            }
        }
        _ => return None,
    };
    let syn::Expr::Path(func_path) = call.func.as_ref() else {
        return None;
    };
    if func_path.qself.is_some() {
        return None;
    }
    let mut segs = func_path.path.segments.iter();
    match (segs.next(), segs.next()) {
        (Some(seg), None) => Some(seg.ident.to_string()),
        _ => None,
    }
}

/// Re-tokenize and re-stringify a fragment or clause into a canonical
/// form for the type-prefix suffix comparison.
#[instrument(level = "debug")]
pub(super) fn normalize_text(text: &str) -> String {
    text.parse::<TokenStream>()
        .map(|stream| canonicalize_type_text(&stream.to_string()))
        .unwrap_or_else(|_| canonicalize_type_text(text.trim()))
}

/// Canonicalize `text` for type-prefix suffix comparison by stripping
/// whitespace and collapsing an elidable trailing comma before `>`.
#[instrument(level = "debug")]
pub(super) fn canonicalize_type_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .replace(",>", ">")
}

#[instrument(level = "debug", skip(tokens))]
pub(in crate::etiquettes::antipatterns::contract_bounds) fn normalize_tokens(
    tokens: TokenStream,
) -> String {
    tokens.to_string()
}

/// Split a flat top-level token sequence on top-level commas.
#[instrument(level = "debug", skip(tokens))]
pub(in crate::etiquettes::antipatterns::contract_bounds) fn split_top_level_commas(
    tokens: &[TokenTree],
) -> Vec<Vec<TokenTree>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    for tt in tokens {
        if let TokenTree::Punct(punct) = tt
            && punct.as_char() == ','
        {
            segments.push(std::mem::take(&mut current));
            continue;
        }
        current.push(tt.clone());
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}
