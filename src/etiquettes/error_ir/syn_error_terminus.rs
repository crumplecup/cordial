//! Detect a `syn::Error` built as the result of a converter or payload
//! expression -- the compliance layer's signal for "this is a proc-macro
//! crate's own diagnostic terminus, not a dropped chain."

use syn::{Expr, ExprCall, Stmt};

use tracing::instrument;
/// Whether `expr` ultimately constructs a `syn::Error` -- `Error::new`,
/// `Error::new_spanned`, or `Error::new2`, either fully qualified
/// (`syn::Error::new(...)`) or bare via a local `use syn::Error` import
/// (`Error::new_spanned(...)`). Textual, not type-resolved, matching how
/// `FOREIGN_TYPE_RULES` already infers foreign types from call shape
/// rather than symbol resolution: `new_spanned`/`new2` are distinctive
/// enough to `syn::Error` that a bare match is safe, but the bare,
/// unqualified `Error::new` is common enough on unrelated local error
/// types that only the `syn::`-qualified form is recognized for it.
#[instrument(level = "debug", skip(expr))]
pub(super) fn builds_syn_error(expr: &Expr) -> bool {
    match expr {
        Expr::Call(call) => constructor_builds_syn_error(call),
        Expr::Closure(closure) => builds_syn_error(&closure.body),
        Expr::Block(block) => matches!(
            block.block.stmts.last(),
            Some(Stmt::Expr(tail, _)) if builds_syn_error(tail)
        ),
        Expr::Paren(paren) => builds_syn_error(&paren.expr),
        Expr::Group(group) => builds_syn_error(&group.expr),
        _ => false,
    }
}

#[instrument(level = "debug", skip(call))]
fn constructor_builds_syn_error(call: &ExprCall) -> bool {
    let Expr::Path(path) = &*call.func else {
        return false;
    };
    let segments: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    let Some((last, rest)) = segments.split_last() else {
        return false;
    };
    if rest.last().is_none_or(|owner| owner != "Error") {
        return false;
    }
    match last.as_str() {
        "new_spanned" | "new2" => true,
        "new" => rest.len() >= 2 && rest[rest.len() - 2] == "syn",
        _ => false,
    }
}
