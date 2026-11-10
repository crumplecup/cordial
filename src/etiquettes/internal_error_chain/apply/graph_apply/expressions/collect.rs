//! Expression sites projected from the canonical source database tree.
use super::super::syntax;
use super::{GraphExprFacts, sites};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, ast};
use std::collections::BTreeMap;
use tracing::instrument;

impl GraphExprFacts {
    #[instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "GraphExprFacts")]
    pub(crate) fn collect_native(file: &DatabaseSource<'_>) -> Self {
        let syntax = file.error_apply_macro_syntax();
        let mut facts = Self { events: Vec::new() };
        collect_node(
            &mut facts,
            file.tree().syntax(),
            None,
            &BTreeMap::new(),
            &syntax,
        );
        facts
    }
}

#[instrument(level = "trace", skip(facts, node, owner, params, syntax))]
fn collect_node(
    facts: &mut GraphExprFacts,
    node: &SyntaxNode,
    owner: Option<&str>,
    params: &BTreeMap<String, std::collections::BTreeSet<String>>,
    syntax: &crate::loader::DatabaseMacroSyntax,
) {
    if matches!(
        node.kind(),
        SyntaxKind::TOKEN_TREE
            | SyntaxKind::CFG_META
            | SyntaxKind::CFG_ATTR_META
            | SyntaxKind::UNSAFE_META
    ) {
        return;
    }
    let new_owner = ast::Impl::cast(node.clone()).map(|item| {
        item.self_ty()
            .and_then(syntax::type_name)
            .unwrap_or_default()
    });
    let owner = new_owner.as_deref().or(owner);
    let new_params = ast::Fn::cast(node.clone())
        .filter(|function| {
            !function.syntax().parent().is_some_and(|parent| {
                ast::ExternItemList::can_cast(parent.kind())
                    || parent
                        .parent()
                        .is_some_and(|node| ast::Trait::can_cast(node.kind()))
            })
        })
        .map(|function| syntax::parameters(&function));
    let params = new_params.as_ref().unwrap_or(params);
    sites::collect(facts, node, owner, params, syntax);
    for child in node.children() {
        collect_node(facts, &child, owner, params, syntax);
    }
}
