//! Context labels for attribute and file-policy owners.
use super::type_label;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasName},
};

#[tracing::instrument(level = "trace", skip(node))]
#[cfg(any(feature = "cfg_hygiene", feature = "cfg_scatter"))]
pub(in crate::ir::source_policy::native) fn label(node: &SyntaxNode) -> Option<String> {
    if let Some(item) = ast::Impl::cast(node.clone()) {
        return Some(format!(
            "impl {}",
            item.self_ty().map(type_label).unwrap_or_else(|| "?".into())
        ));
    }
    macro_rules! named {
        ($($ty:ident => $prefix:literal),* $(,)?) => { $(
            if let Some(item) = ast::$ty::cast(node.clone()) {
                return item.name().map(|name| format!(concat!($prefix, " {}"), name.text()));
            }
        )* };
    }
    named!(Fn => "fn", Struct => "struct", Enum => "enum", Trait => "trait", Const => "const", Static => "static", TypeAlias => "type");
    None
}

#[tracing::instrument(level = "trace", skip(owner, prefix))]
#[cfg(feature = "cfg_hygiene")]
pub(in crate::ir::source_policy::native) fn hygiene(
    owner: &SyntaxNode,
    prefix: &[String],
) -> String {
    let mut modules = prefix.to_vec();
    let mut items = Vec::new();
    for node in lineage(owner) {
        if let Some(module) = ast::Module::cast(node.clone()) {
            if node != *owner
                && let Some(name) = module.name()
            {
                modules.push(name.text().to_string());
            }
        } else if let Some(label) = label(&node) {
            items.push(label);
        }
    }
    modules.extend(items);
    joined(modules)
}

/// Allows include their owner's function/impl; scatter describes the parent.
#[tracing::instrument(level = "trace", skip(owner, prefix))]
#[cfg(any(feature = "allows", feature = "cfg_scatter"))]
pub(in crate::ir::source_policy::native) fn site(
    owner: &SyntaxNode,
    prefix: &[String],
    include_owner: bool,
) -> String {
    let mut modules = prefix.to_vec();
    let mut impl_type = None;
    let mut functions = Vec::new();
    for node in lineage(owner) {
        if let Some(module) = ast::Module::cast(node.clone()) {
            if node != *owner
                && let Some(name) = module.name()
            {
                modules.push(name.text().to_string());
            }
        } else if node != *owner || include_owner {
            if let Some(item) = ast::Impl::cast(node.clone()) {
                impl_type = item.self_ty().map(type_label);
            }
            if let Some(item) = ast::Fn::cast(node.clone()) {
                let in_trait = item
                    .syntax()
                    .parent()
                    .and_then(|parent| parent.parent())
                    .is_some_and(|parent| ast::Trait::can_cast(parent.kind()));
                if (!include_owner || !in_trait)
                    && let Some(name) = item.name()
                {
                    functions.push(name.text().to_string());
                }
            }
        }
    }
    modules.extend(impl_type);
    modules.extend(functions);
    joined(modules)
}

#[tracing::instrument(level = "trace", skip(node))]
fn lineage(node: &SyntaxNode) -> Vec<SyntaxNode> {
    let mut nodes: Vec<_> = node.ancestors().collect();
    nodes.reverse();
    nodes
}

#[tracing::instrument(level = "trace", skip(parts))]
pub(in crate::ir::source_policy::native) fn joined(parts: Vec<String>) -> String {
    if parts.is_empty() {
        "<crate>".into()
    } else {
        parts.join("::")
    }
}

#[cfg(feature = "modularity")]
#[tracing::instrument(level = "trace", skip(node, prefix))]
pub(in crate::ir::source_policy::native) fn function_context(
    node: &SyntaxNode,
    prefix: &[String],
) -> String {
    let mut modules = prefix.to_vec();
    let mut owner = None;
    let mut functions = Vec::new();
    for ancestor in lineage(node) {
        if let Some(module) = ast::Module::cast(ancestor.clone()) {
            modules.extend(module.name().map(|name| name.text().to_string()));
        }
        if let Some(item) = ast::Impl::cast(ancestor.clone()) {
            owner = item.self_ty().map(type_label);
        }
        if let Some(item) = ast::Trait::cast(ancestor.clone()) {
            owner = item.name().map(|name| name.text().to_string());
        }
        if let Some(item) = ast::Fn::cast(ancestor) {
            functions.extend(item.name().map(|name| name.text().to_string()));
        }
    }
    modules.extend(owner);
    modules.extend(functions);
    joined(modules)
}
