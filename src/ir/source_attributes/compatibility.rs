//! Attribute projection from the shared frontend during migration.

use syn::{Attribute, Meta, Path as SynPath};
use tracing::instrument;

/// `true` for `#[instrument]`, `#[tracing::instrument]`, and the same
/// wrapped in `#[cfg_attr(<predicate>, ...)]` -- apply writes the gated
/// form for verifier crates (`not(kani)`, etc.), and a later quality
/// run has to treat that as already instrumented or every gated
/// function stays an open missing-instrument finding forever.
#[instrument(level = "trace", skip(attr))]
pub fn is_instrument_attr(attr: &Attribute) -> bool {
    match &attr.meta {
        Meta::Path(path) => path_is_instrument(path),
        Meta::List(list) => path_is_instrument(&list.path) || cfg_attr_inner_is_instrument(list),
        Meta::NameValue(value) => path_is_instrument(&value.path),
    }
}

/// `true` when `attr` is `#[cfg_attr(.., instrument)]` / `tracing::instrument`
/// — the gated form apply writes for verifier crates. Bare `#[instrument]`
/// is not gated: a Kani (etc.) build will still expand it.
#[cfg(feature = "_attribute_scan")]
#[instrument(level = "trace", skip(attr))]
pub fn is_gated_instrument_attr(attr: &Attribute) -> bool {
    match &attr.meta {
        Meta::List(list) => cfg_attr_inner_is_instrument(list),
        _ => false,
    }
}

#[instrument(level = "debug", skip(path))]
fn path_is_instrument(path: &SynPath) -> bool {
    if path.is_ident("instrument") {
        return true;
    }
    path.segments.len() == 2
        && path.segments[0].ident == "tracing"
        && path.segments[1].ident == "instrument"
}

#[instrument(level = "debug", skip(list))]
fn cfg_attr_inner_is_instrument(list: &syn::MetaList) -> bool {
    if !list.path.is_ident("cfg_attr") {
        return false;
    }
    cfg_attr_inner_path(&list.tokens).is_some_and(|path| path_is_instrument(&path))
}

/// Path of `cfg_attr`'s inner attribute (`tracing::instrument` in
/// `cfg_attr(not(kani), tracing::instrument(level = "debug"))`).
#[instrument(level = "debug", skip(tokens))]
fn cfg_attr_inner_path(tokens: &proc_macro2::TokenStream) -> Option<SynPath> {
    let mut seen_comma = false;
    let mut inner = Vec::new();
    for tree in tokens.clone() {
        match &tree {
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == ',' && !seen_comma => {
                seen_comma = true;
            }
            _ if seen_comma => inner.push(tree),
            _ => {}
        }
    }
    if !seen_comma {
        return None;
    }
    let path_tokens: proc_macro2::TokenStream = inner
        .into_iter()
        .take_while(|tree| !matches!(tree, proc_macro2::TokenTree::Group(_)))
        .collect();
    syn::parse2(path_tokens).ok()
}

/// `true` when `attrs` carry `#[cfg(test)]`, so a scan can skip test-only code.
#[instrument(level = "trace", skip(attrs))]
pub fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        list.tokens.to_string().replace(' ', "") == "test"
    })
}
