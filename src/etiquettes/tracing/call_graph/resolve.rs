use std::collections::{HashMap, HashSet};

use syn::Expr;
use syn::visit::Visit;
use tracing::instrument;

use super::collect::CollectedFn;

#[instrument(level = "debug", skip(collected))]
pub(super) fn never_instrument_from_calls(
    collected: &[CollectedFn],
) -> HashMap<String, HashSet<String>> {
    let registry = build_registry(collected);
    let callers_of = build_callers_of(collected, &registry);
    let excluded = proof_only_fixed_point(collected, &callers_of);
    excluded
        .into_iter()
        .fold(HashMap::new(), |mut never_instrument, index| {
            let id = collected[index].id();
            never_instrument
                .entry(id.crate_name().clone())
                .or_insert_with(HashSet::new)
                .insert(id.qualified_name().clone());
            never_instrument
        })
}

/// Registry: call key -> every collected function whose own
/// call_keys include it (workspace-wide, deliberately not crate-
/// scoped -- a call site's own text doesn't name which crate its
/// callee lives in).
#[instrument(level = "debug", skip(collected))]
fn build_registry(collected: &[CollectedFn]) -> HashMap<&str, Vec<usize>> {
    let mut registry: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, function) in collected.iter().enumerate() {
        for key in function.call_keys() {
            registry.entry(key.as_str()).or_default().push(index);
        }
    }
    registry
}

#[instrument(level = "debug", skip(collected, registry))]
fn build_callers_of(
    collected: &[CollectedFn],
    registry: &HashMap<&str, Vec<usize>>,
) -> HashMap<usize, HashSet<usize>> {
    let mut callers_of: HashMap<usize, HashSet<usize>> = HashMap::new();
    for (caller_index, function) in collected.iter().enumerate() {
        let Some(body) = function.body() else {
            continue;
        };
        let mut call_visitor = CallSiteVisitor { calls: Vec::new() };
        call_visitor.visit_block(body);
        for key in call_visitor.calls {
            let Some(candidates) = registry.get(key.as_str()) else {
                continue;
            };
            // Ambiguous (>1 workspace-wide definition sharing this
            // exact key) -- don't guess which one a bare/short name
            // meant; an unresolved call never produces a false
            // exclusion, only a missed one.
            let [callee_index] = candidates[..] else {
                continue;
            };
            if callee_index != caller_index {
                callers_of
                    .entry(callee_index)
                    .or_default()
                    .insert(caller_index);
            }
        }
    }
    callers_of
}

#[instrument(level = "debug", skip(collected, callers_of))]
fn proof_only_fixed_point(
    collected: &[CollectedFn],
    callers_of: &HashMap<usize, HashSet<usize>>,
) -> HashSet<usize> {
    let mut excluded: HashSet<usize> = collected
        .iter()
        .enumerate()
        .filter(|(_, function)| function.ancestor_seed())
        .map(|(index, _)| index)
        .collect();
    loop {
        let mut added_any = false;
        for (index, callers) in callers_of {
            if excluded.contains(index) {
                continue;
            }
            if !callers.is_empty() && callers.iter().all(|caller| excluded.contains(caller)) {
                excluded.insert(*index);
                added_any = true;
            }
        }
        if !added_any {
            break;
        }
    }
    excluded
}

/// Macro names whose arguments are plain expressions worth looking
/// inside -- `syn` never expands macros, so a call wrapped in one of
/// these (`assert!(Type::method(..))`, the real shape almost every
/// `Ensures`/`Requires` call site in `amenable_kani` actually uses) is
/// otherwise invisible to a syn-only walk entirely. `assert!`/
/// `debug_assert!` take one leading condition expression (an optional
/// message follows); the `_eq`/`_ne` family take two leading value
/// expressions.
const TRANSPARENT_ASSERT_MACROS: &[&str] = &["assert", "debug_assert"];
const TRANSPARENT_COMPARE_MACROS: &[&str] = &[
    "assert_eq",
    "assert_ne",
    "debug_assert_eq",
    "debug_assert_ne",
];

/// Collects every unambiguous, name-resolvable call key
/// (`Type::method`/`Trait::method`/`bare_fn`) reached anywhere in one
/// function body -- `receiver.method(..)` calls are not collected at
/// all (see the module doc comment).
struct CallSiteVisitor {
    calls: Vec<String>,
}

impl<'ast> Visit<'ast> for CallSiteVisitor {
    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let Expr::Path(expr_path) = node.func.as_ref() {
            let segments = &expr_path.path.segments;
            match segments.len() {
                1 => self.calls.push(segments[0].ident.to_string()),
                len if len >= 2 => {
                    let type_seg = &segments[len - 2].ident;
                    let method_seg = &segments[len - 1].ident;
                    self.calls.push(format!("{type_seg}::{method_seg}"));
                }
                _ => {}
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    // `syn` never expands macros, and `visit_expr_macro` alone misses a
    // macro invocation written as a whole statement (`assert!(..);`
    // parses as `Stmt::Macro`, not `Stmt::Expr(Expr::Macro(..), ..)`) --
    // `visit_macro` is the one method both forms delegate to, so it's
    // the only hook that reliably sees every macro call site.
    #[instrument(level = "trace", skip(self, node))]
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let name = node.path.get_ident().map(ToString::to_string);
        let take = if name
            .as_deref()
            .is_some_and(|n| TRANSPARENT_ASSERT_MACROS.contains(&n))
        {
            1
        } else if name
            .as_deref()
            .is_some_and(|n| TRANSPARENT_COMPARE_MACROS.contains(&n))
        {
            2
        } else {
            0
        };
        if take > 0
            && let Ok(args) = syn::parse::Parser::parse2(
                syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated,
                node.tokens.clone(),
            )
        {
            for arg in args.iter().take(take) {
                self.visit_expr(arg);
            }
        }
        syn::visit::visit_macro(self, node);
    }
}
