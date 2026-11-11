//! Project conservative recording evidence from the database-issued syntax.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};
use tracing::instrument;

/// The names the parameters bind, `self` first. Whether each can be recorded is
/// the database's verdict, not anything read from its written type.
#[instrument(level = "debug", skip(function))]
pub(super) fn parameter_names(function: &ast::Fn) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(list) = function.param_list() {
        if list.self_param().is_some() {
            names.push("self".into());
        }
        for pattern in list.params().filter_map(|param| param.pat()) {
            names.extend(
                pattern
                    .syntax()
                    .descendants()
                    .filter_map(ast::IdentPat::cast)
                    .filter_map(|binding| binding.name())
                    .map(|name| name.text().to_string()),
            );
        }
    }
    names
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn returns_result(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::PathType(ty) => ty
            .path()
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .is_some_and(|name| name.text().ends_with("Result")),
        ast::Type::RefType(ty) => ty.ty().is_some_and(|ty| returns_result(&ty)),
        ast::Type::ParenType(ty) => ty.ty().is_some_and(|ty| returns_result(&ty)),
        _ => false,
    }
}
