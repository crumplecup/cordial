//! Visibility queries over the loader-projected module tree.
use crate::ir::VisibilityModule as ModuleNode;
use tracing::instrument;
#[instrument(level = "debug", skip(node))]
pub(super) fn external_name_count(node: &ModuleNode) -> usize {
    let mut n = node.leaf_pub();
    for child in node.children() {
        if child.declared_vis().is_unrestricted_pub() {
            n += external_name_count(child);
        }
    }
    n
}

#[instrument(level = "debug", skip(node))]
pub(super) fn public_path_mods(node: &ModuleNode) -> Vec<&ModuleNode> {
    let mut out = Vec::new();
    for child in node.children() {
        if child.declared_vis().is_unrestricted_pub() && child.ancestors_all_pub() {
            out.push(child);
            out.extend(public_path_mods(child));
        }
    }
    out
}
