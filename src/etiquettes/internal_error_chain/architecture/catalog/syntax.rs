//! Exact syntax contracts shared by architecture fact collectors.
pub(in crate::etiquettes::internal_error_chain::architecture::catalog) use crate::ir::{
    native_error_path_label as path_label, native_error_type_label as type_label,
};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasAttrs, HasGenericArgs},
};

#[tracing::instrument(level = "trace", skip(owner))]
pub(super) fn cfg_test(owner: &impl HasAttrs) -> bool {
    owner.attrs().any(|attr| match attr.meta() {
        Some(ast::Meta::CfgMeta(meta)) => meta
            .cfg_predicate()
            .is_some_and(|p| compact(p.syntax()) == "test"),
        _ => false,
    })
}
#[tracing::instrument(level = "trace", skip(node))]
pub(super) fn compact(node: &ra_ap_syntax::SyntaxNode) -> String {
    node.descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .map(|t| t.text().to_string())
        .collect()
}
#[tracing::instrument(level = "trace", skip(ty))]
pub(in crate::etiquettes::internal_error_chain::architecture::catalog) fn simple_type(
    ty: &ast::Type,
) -> Option<String> {
    let ast::Type::PathType(ty) = ty else {
        return None;
    };
    let path = ty.path()?;
    simple_path(&path)
}
#[tracing::instrument(level = "trace", skip(path))]
pub(super) fn simple_path(path: &ast::Path) -> Option<String> {
    let segment = path.segment()?;
    (path.qualifier().is_none()
        && path.coloncolon_token().is_none()
        && segment.coloncolon_token().is_none()
        && segment.generic_arg_list().is_none()
        && segment.parenthesized_arg_list().is_none())
    .then(|| segment.name_ref().map(|n| n.text().to_string()))
    .flatten()
}
#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn peel(mut expr: ast::Expr) -> ast::Expr {
    loop {
        expr = match expr {
            ast::Expr::ParenExpr(ref p) => match p.expr() {
                Some(e) => e,
                None => return expr,
            },
            _ => return expr,
        };
    }
}
#[tracing::instrument(level = "trace", skip(call))]
pub(super) fn one_arg(call: &ast::CallExpr) -> Option<ast::Expr> {
    let mut args = call.arg_list()?.args();
    let first = args.next()?;
    args.next().is_none().then_some(first)
}
#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn callee(expr: &ast::Expr) -> Option<(Option<String>, String)> {
    let ast::Expr::PathExpr(path) = peel(expr.clone()) else {
        return None;
    };
    Some(path_pair(&path.path()?))
}
#[tracing::instrument(level = "trace", skip(path))]
pub(super) fn path_pair(path: &ast::Path) -> (Option<String>, String) {
    let label = path_label(path);
    let mut parts = label.rsplit("::");
    let name = parts.next().unwrap_or("").to_string();
    (parts.next().map(str::to_string), name)
}
