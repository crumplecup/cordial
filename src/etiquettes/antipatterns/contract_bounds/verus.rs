//! Verus `requires`/`ensures` clauses inside `verus! { ... }`.

use std::path::Path;

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::ItemMacro;
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::etiquettes::antipatterns::types::AntipatternSiteRecord;
use crate::loader::module_path_from_src_file;

use super::index::{ContractIndex, is_trivial, normalize_tokens, split_top_level_commas};
use super::{make_finding, site_context};

#[instrument(level = "debug", skip(source, file, index), err(level = "warn"))]
pub(super) fn scan_verus_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    index: &ContractIndex,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut findings = Vec::new();

    for item in &syntax.items {
        let syn::Item::Macro(ItemMacro { mac, .. }) = item else {
            continue;
        };
        if mac
            .path
            .segments
            .last()
            .is_none_or(|seg| seg.ident != "verus")
        {
            continue;
        }
        let mut clauses = Vec::new();
        walk_verus_tokens(mac.tokens.clone(), &mut clauses);
        for (kind, clause, harness) in clauses {
            let normalized = normalize_tokens(clause.clone());
            if is_trivial(&normalized) || index.matches_named_call("verus", kind, clause.clone()) {
                continue;
            }
            let line = clause
                .clone()
                .into_iter()
                .next()
                .map(|tt| tt.span().start().line as u32)
                .unwrap_or(0);
            let leaf = if harness.is_empty() {
                "verus!".to_string()
            } else {
                format!("verus!::{harness}")
            };
            let context = site_context(&module_prefix, &leaf);
            findings.push(make_finding(context, file, line, &normalized));
        }
    }

    Ok(findings)
}

/// Recursively find every `requires`/`ensures` keyword in `tokens`, descending
/// into every nested `Group` (function bodies, parenthesized expressions,
/// ...) since a whole `verus!{}` file may contain many functions. For each
/// keyword found, the clause list runs from the token right after it up to
/// the next top-level brace group (the function body starting) or the next
/// `requires`/`ensures` keyword, split on top-level commas into individual
/// clauses.
///
/// Also tracks the enclosing `fn`'s name at the current nesting level (a
/// `requires`/`ensures` clause always sits directly after its own
/// function's signature, at the same top-level token sequence as the `fn`
/// keyword — never inside a nested `Group`), and attaches it to each
/// clause so matching can be scoped to "this clause's own site."
#[instrument(level = "debug", skip(tokens, out))]
fn walk_verus_tokens(tokens: TokenStream, out: &mut Vec<(&'static str, TokenStream, String)>) {
    let items: Vec<TokenTree> = tokens.into_iter().collect();
    let mut i = 0;
    let mut current_fn = String::new();
    while i < items.len() {
        match &items[i] {
            TokenTree::Ident(ident) if ident == "fn" => {
                if let Some(TokenTree::Ident(name)) = items.get(i + 1) {
                    current_fn = name.to_string();
                }
                i += 1;
            }
            TokenTree::Ident(ident) if ident == "requires" || ident == "ensures" => {
                let kind = if ident == "requires" {
                    "requires"
                } else {
                    "ensures"
                };
                let mut j = i + 1;
                while j < items.len() {
                    match &items[j] {
                        TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => break,
                        TokenTree::Ident(next) if next == "requires" || next == "ensures" => break,
                        _ => j += 1,
                    }
                }
                for segment in split_top_level_commas(&items[i + 1..j]) {
                    if segment.is_empty() {
                        continue;
                    }
                    out.push((kind, segment.into_iter().collect(), current_fn.clone()));
                }
                i = j;
            }
            TokenTree::Group(group) => {
                walk_verus_tokens(group.stream(), out);
                i += 1;
            }
            _ => i += 1,
        }
    }
}
