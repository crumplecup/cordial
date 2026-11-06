//! Verus `requires`/`ensures` clauses inside `verus! { ... }`.

use std::path::Path;

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use tracing::instrument;

use crate::error::CordialResult;

use super::ContractClause;
use super::index::{
    is_builtin_contract_inspection, is_trivial, normalize_tokens, split_top_level_commas,
};

use super::{make_finding, site_context};

#[instrument(level = "debug", skip(tokens, file), err(level = "warn"))]
pub(super) fn collect_payload(
    tokens: TokenStream,
    file: &Path,
    module_prefix: &[String],
) -> CordialResult<Vec<ContractClause>> {
    let mut findings = Vec::new();
    let mut clause_groups = Vec::new();
    walk_verus_tokens(tokens, &mut clause_groups);
    for (kind, clauses, harness) in clause_groups {
        for (idx, clause) in clauses.iter().cloned().enumerate() {
            let normalized = normalize_tokens(clause.clone());
            if is_trivial(&normalized) || is_builtin_contract_inspection(kind, clause.clone()) {
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
            let context = site_context(module_prefix, &leaf);
            let sibling_names = clauses
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != idx)
                .filter_map(|(_, sibling)| ContractClause::sibling_name(sibling.clone()))
                .collect();
            findings.push(ContractClause::collect(
                make_finding(context, file, line, &normalized)?,
                "verus",
                kind,
                clause,
                sibling_names,
            ));
        }
    }

    Ok(findings)
}

/// Recursively find every `requires`/`ensures` keyword in `tokens`, descending
/// into every nested `Group` (function bodies, parenthesized expressions,
/// ...) since a whole `verus!{}` file may contain many functions. For each
/// keyword found, the clause list runs from the token right after it up to
/// whichever comes first: the next top-level brace group (an ordinary
/// function's body starting), a top-level bare `;` (an `assume_specification`
/// or other body-less declaration ending — these have no brace group at
/// all), or the next `requires`/`ensures` keyword. Split on top-level commas
/// into individual clauses.
///
/// The `;` stop case matters: without it, an `assume_specification`'s
/// clause list runs straight through its terminating semicolon and keeps
/// consuming whatever comes next (the following item's doc-comment
/// attributes, then its own signature) as if it were more of the same
/// clause list, manufacturing a garbage "clause" out of unrelated tokens.
///
/// Also tracks the enclosing `fn`'s name at the current nesting level (a
/// `requires`/`ensures` clause always sits directly after its own
/// function's signature, at the same top-level token sequence as the `fn`
/// keyword — never inside a nested `Group`), and attaches it to each
/// clause so matching can be scoped to "this clause's own site."
///
/// Every clause split from one `requires`/`ensures` occurrence is kept
/// together as a single group (rather than flattened into the output one
/// clause at a time) so a caller can recognize a raw equation that only
/// restates a *sibling* clause in the same list — see
/// [`ContractClause`].
/// Whether `items[idx]` is immediately preceded by a `.` — a dotted
/// method call (`H::default.ensures((), result)`, Verus's own builtin
/// function-item contract-inspection method) spells its method name
/// `ensures`/`requires` identically to the real clause-list keyword,
/// but is never one: a genuine `requires`/`ensures` keyword always
/// follows a function signature directly, never a `.`.
#[instrument(level = "debug", skip(items))]
fn preceded_by_dot(items: &[TokenTree], idx: usize) -> bool {
    idx > 0
        && matches!(
            &items[idx - 1],
            TokenTree::Punct(punct) if punct.as_char() == '.'
        )
}

#[instrument(level = "debug", skip(tokens, out))]
fn walk_verus_tokens(tokens: TokenStream, out: &mut Vec<(&'static str, Vec<TokenStream>, String)>) {
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
            TokenTree::Ident(ident)
                if (ident == "requires" || ident == "ensures") && !preceded_by_dot(&items, i) =>
            {
                let kind = if ident == "requires" {
                    "requires"
                } else {
                    "ensures"
                };
                let mut j = i + 1;
                while j < items.len() {
                    match &items[j] {
                        TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => break,
                        TokenTree::Punct(punct) if punct.as_char() == ';' => break,
                        TokenTree::Ident(next)
                            if (next == "requires" || next == "ensures")
                                && !preceded_by_dot(&items, j) =>
                        {
                            break;
                        }
                        _ => j += 1,
                    }
                }
                let group: Vec<TokenStream> = split_top_level_commas(&items[i + 1..j])
                    .into_iter()
                    .filter(|segment| !segment.is_empty())
                    .map(|segment| segment.into_iter().collect())
                    .collect();
                if !group.is_empty() {
                    out.push((kind, group, current_fn.clone()));
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
