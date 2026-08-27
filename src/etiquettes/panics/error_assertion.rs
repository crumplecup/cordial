//! Detects `let name = <expr>.expect_err(..)/.unwrap_err(); ...
//! assert*!(.. name ..)` -- a real, structural "asserting facts against
//! an error" pattern: the call's whole point is to extract the error
//! value and inspect it (`assert_eq!(error, TransferError::
//! NegativeAmount(-1))`, `assert_eq!(err.offending_path(), bad_path)`,
//! ...), not to discard or propagate a setup failure. Distinct from a
//! bare `.unwrap_err()`/`.expect_err()` with no follow-up use, which
//! stays flagged exactly like its `.unwrap()`/`.expect()` counterpart
//! (see `tests/panics_etiquette.rs`'s own
//! `unwrap_err_and_expect_err_are_flagged_the_same_as_their_ok_
//! counterparts` -- that test's fixture never uses the bound value
//! again, so it's unaffected by this).
//!
//! Deliberately narrow, matching the real shapes actually found in this
//! codebase's own test suite: the call must be the *direct* init
//! expression of a `let name = ...;` binding a single identifier (no
//! destructuring, no `?`/method chaining beyond the one call). A value
//! derived from that binding via exactly one further `let other = ...
//! name ...;` is also tracked (`let rendered = error.to_string();
//! assert!(rendered.contains(..))`'s own real shape), but no deeper --
//! the safe direction for a pattern feeding an exemption is to
//! under-match, not over-match.

use std::collections::HashSet;

use proc_macro2::TokenTree;
use syn::spanned::Spanned;
use syn::{Block, Expr, Local, Macro, Pat, Stmt};

use tracing::instrument;

/// Line numbers of `.expect_err(..)`/`.unwrap_err()` calls in `block`
/// whose bound result (or a value derived from it in one more `let`)
/// is used inside a later `assert!`/`assert_eq!`/`assert_ne!` (or
/// `debug_` twin) in the same block -- see the module doc.
#[instrument(level = "debug", skip(block))]
pub(super) fn error_assertion_lines(block: &Block) -> HashSet<u32> {
    let mut exempt = HashSet::new();
    for (index, stmt) in block.stmts.iter().enumerate() {
        let Some((line, name)) = error_assertion_binding(stmt) else {
            continue;
        };
        let mut reachable: HashSet<String> = HashSet::new();
        reachable.insert(name);
        for later in &block.stmts[index + 1..] {
            if let Some((bound_name, uses)) = local_binding_uses(later) {
                if uses.iter().any(|used| reachable.contains(used)) {
                    reachable.insert(bound_name);
                }
                continue;
            }
            if let Some(uses) = assert_macro_uses(later)
                && uses.iter().any(|used| reachable.contains(used))
            {
                exempt.insert(line);
                break;
            }
        }
    }
    exempt
}

/// Whether `stmt` is `let name = <expr>.expect_err(..)/.unwrap_err();`,
/// returning the call's own line and the bound name.
#[instrument(level = "trace", skip(stmt), ret)]
fn error_assertion_binding(stmt: &Stmt) -> Option<(u32, String)> {
    let Stmt::Local(local) = stmt else {
        return None;
    };
    let name = simple_binding_name(local)?;
    let init = local.init.as_ref()?;
    let Expr::MethodCall(call) = init.expr.as_ref() else {
        return None;
    };
    if call.method != "expect_err" && call.method != "unwrap_err" {
        return None;
    }
    // Must match check_method_call's own line computation exactly
    // (the whole call expression's span, which starts at the
    // *receiver*'s own beginning for a multi-line receiver -- not
    // call.method.span(), just the method identifier) or the lookup
    // silently misses for any receiver spanning more than one line.
    Some((call.span().start().line as u32, name))
}

/// Whether `stmt` is `let name = <expr>;` for a simple identifier
/// binding, returning the bound name and every identifier appearing in
/// the initializer's own tokens.
#[instrument(level = "trace", skip(stmt), ret)]
fn local_binding_uses(stmt: &Stmt) -> Option<(String, HashSet<String>)> {
    let Stmt::Local(local) = stmt else {
        return None;
    };
    let name = simple_binding_name(local)?;
    let init = local.init.as_ref()?;
    let expr = init.expr.as_ref();
    let mut uses = HashSet::new();
    collect_idents(&quote::quote!(#expr), &mut uses);
    Some((name, uses))
}

/// Whether `stmt` is an `assert!`/`assert_eq!`/`assert_ne!`/`debug_`
/// twin invocation, returning every identifier in its own arguments.
#[instrument(level = "trace", skip(stmt), ret)]
fn assert_macro_uses(stmt: &Stmt) -> Option<HashSet<String>> {
    let mac = match stmt {
        Stmt::Macro(stmt_macro) => &stmt_macro.mac,
        Stmt::Expr(Expr::Macro(expr_macro), _) => &expr_macro.mac,
        _ => return None,
    };
    is_assert_macro(mac).then(|| {
        let mut uses = HashSet::new();
        collect_idents(&mac.tokens, &mut uses);
        uses
    })
}

#[instrument(level = "trace", skip(local), ret)]
fn simple_binding_name(local: &Local) -> Option<String> {
    match &local.pat {
        Pat::Ident(pat_ident) => Some(pat_ident.ident.to_string()),
        _ => None,
    }
}

#[instrument(level = "trace", skip(mac), ret)]
fn is_assert_macro(mac: &Macro) -> bool {
    mac.path.segments.last().is_some_and(|segment| {
        matches!(
            segment.ident.to_string().as_str(),
            "assert"
                | "assert_eq"
                | "assert_ne"
                | "debug_assert"
                | "debug_assert_eq"
                | "debug_assert_ne"
        )
    })
}

#[instrument(level = "trace", skip(tokens, out))]
fn collect_idents(tokens: &proc_macro2::TokenStream, out: &mut HashSet<String>) {
    for tree in tokens.clone() {
        match tree {
            TokenTree::Ident(ident) => {
                out.insert(ident.to_string());
            }
            TokenTree::Group(group) => collect_idents(&group.stream(), out),
            _ => {}
        }
    }
}
