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
