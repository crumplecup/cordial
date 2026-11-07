//! Local static-string aliases, without resolving across lexical scopes.
use super::syntax;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasAttrs, HasGenericParams, HasName},
};
use std::collections::BTreeMap;

#[tracing::instrument(level = "trace", skip(node))]
pub(super) fn collect(node: &SyntaxNode) -> BTreeMap<String, ast::Type> {
    let mut groups = BTreeMap::<String, Vec<ast::TypeAlias>>::new();
    for item in node.children().filter_map(ast::TypeAlias::cast) {
        if let Some(name) = item.name() {
            groups
                .entry(name.text().to_string())
                .or_default()
                .push(item);
        }
    }
    groups
        .into_iter()
        .filter_map(|(name, items)| {
            let [item] = items.as_slice() else {
                return None;
            };
            let ty = item.ty()?;
            let ast::Type::RefType(reference) = &ty else {
                return None;
            };
            (item
                .generic_param_list()
                .is_none_or(|p| p.generic_params().next().is_none())
                && item
                    .attrs()
                    .all(|a| matches!(a.simple_name().as_deref(), Some("doc" | "cfg")))
                && reference.mut_token().is_none()
                && reference
                    .lifetime()
                    .is_some_and(|l| l.syntax().text() == "'static")
                && reference
                    .ty()
                    .and_then(|t| syntax::simple_type(&t))
                    .as_deref()
                    == Some("str"))
            .then_some((name, ty))
        })
        .collect()
}
