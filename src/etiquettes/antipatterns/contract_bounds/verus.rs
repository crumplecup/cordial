//! Verus `requires`/`ensures` clauses inside `verus! { ... }`.

use std::path::Path;

use tracing::instrument;

use crate::error::CordialResult;

use super::ContractClause;
use super::index::{is_builtin_contract_inspection, is_trivial};
use super::token_tree::{Delimiter, Token, TokenKind, render, split_top_level_commas};

use super::{make_finding, site_context};

#[instrument(level = "debug", skip(tokens, file), err(level = "warn"))]
pub(super) fn collect_payload(
    tokens: &[Token],
    file: &Path,
    module_prefix: &[String],
) -> CordialResult<Vec<ContractClause>> {
    let mut findings = Vec::new();
    let mut clause_groups = Vec::new();
    walk_verus_tokens(tokens, &mut clause_groups);
    for (kind, clauses, harness) in clause_groups {
        for (idx, clause) in clauses.iter().enumerate() {
            let normalized = render(clause);
            if is_trivial(&normalized) || is_builtin_contract_inspection(kind, clause) {
                continue;
            }
            let line = clause.first().map_or(0, Token::line);
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
                .filter_map(|(_, sibling)| ContractClause::sibling_name(sibling))
                .collect();
            findings.push(ContractClause::collect(
                make_finding(context, file, line, &normalized)?,
                "verus",
                kind,
                clause.clone(),
                sibling_names,
            ));
        }
    }

    Ok(findings)
}

/// Whether `items[idx]` is immediately preceded by a `.` — a dotted
/// method call (`H::default.ensures((), result)`, Verus's own builtin
/// function-item contract-inspection method) spells its method name
/// `ensures`/`requires` identically to the real clause-list keyword,
/// but is never one: a genuine `requires`/`ensures` keyword always
/// follows a function signature directly, never a `.`.
#[instrument(level = "debug", skip(items))]
fn preceded_by_dot(items: &[Token], idx: usize) -> bool {
    idx > 0 && items[idx - 1].is_punct('.')
}

type ClauseGroup = (&'static str, Vec<Vec<Token>>, String);

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
#[instrument(level = "debug", skip(items, out))]
fn walk_verus_tokens(items: &[Token], out: &mut Vec<ClauseGroup>) {
    let mut i = 0;
    let mut current_fn = String::new();
    while i < items.len() {
        match items[i].kind() {
            TokenKind::Ident(ident) if ident == "fn" => {
                if let Some(name) = items.get(i + 1).and_then(Token::ident) {
                    current_fn = name.to_string();
                }
                i += 1;
            }
            TokenKind::Ident(ident)
                if (ident == "requires" || ident == "ensures") && !preceded_by_dot(items, i) =>
            {
                let kind = if ident == "requires" {
                    "requires"
                } else {
                    "ensures"
                };
                let mut j = i + 1;
                while j < items.len() {
                    let ends_list = items[j].group(Delimiter::Brace).is_some()
                        || items[j].is_punct(';')
                        || (matches!(items[j].ident(), Some("requires" | "ensures"))
                            && !preceded_by_dot(items, j));
                    if ends_list {
                        break;
                    }
                    j += 1;
                }
                let group: Vec<Vec<Token>> = split_top_level_commas(&items[i + 1..j])
                    .into_iter()
                    .filter(|segment| !segment.is_empty())
                    .collect();
                if !group.is_empty() {
                    out.push((kind, group, current_fn.clone()));
                }
                i = j;
            }
            TokenKind::Group(_, inner) => {
                walk_verus_tokens(inner, out);
                i += 1;
            }
            _ => i += 1,
        }
    }
}
