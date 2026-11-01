//! File policies collected directly from the canonical database tree.

use super::SourcePolicyInventory;
use crate::loader::{DatabaseSource, module_path_from_src_file};
use crate::{CordialResult, FileSnapshot};
use ra_ap_syntax::{AstNode, SyntaxNode, ast};
use std::path::Path;

#[cfg(feature = "allows")]
mod allows;
#[cfg(feature = "cfg_hygiene")]
mod cfg_names;
mod context;
#[cfg(feature = "cfg_scatter")]
mod scatter;
mod tokens;

impl SourcePolicyInventory {
    #[tracing::instrument(level = "debug", skip(file, self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourcePolicyInventory")]
    pub(crate) fn collect_native_file(
        &mut self,
        file: &DatabaseSource<'_>,
        crate_root: &Path,
    ) -> CordialResult<()> {
        self.crate_root = crate_root.to_path_buf();
        let source_root = crate_root.join("src");
        let test_root = crate_root.join("tests");
        let is_source = file.path().starts_with(&source_root);
        let is_test = file.path().starts_with(&test_root);
        let include_allows = cfg!(feature = "allows")
            && (is_source || is_test)
            && !crate::loader::path_has_fixtures(file.path(), crate_root);
        let include_cfg = is_source && cfg!(any(feature = "cfg_hygiene", feature = "cfg_scatter"));
        if !include_cfg && !include_allows {
            return Ok(());
        }
        let tree_root = if is_source { &source_root } else { &test_root };
        let prefix = module_path_from_src_file(tree_root, file.path());
        self.snapshots.push(FileSnapshot::new(
            file.path().to_path_buf(),
            file.source_digest(),
        ));
        #[cfg(feature = "allows")]
        if include_allows {
            self.allows
                .extend(allows::collect(file, crate_root, &prefix)?);
        }
        if is_source {
            #[cfg(feature = "cfg_hygiene")]
            self.cfg_names
                .extend(cfg_names::collect(file, crate_root, &prefix)?);
            #[cfg(feature = "cfg_scatter")]
            self.cfg_groups
                .extend(scatter::collect(file, crate_root, &prefix)?);
        }
        Ok(())
    }
}

/// Macro payloads are token trees, not attribute-bearing Rust owners.
#[tracing::instrument(level = "trace", skip(node))]
fn outside_macro(node: &SyntaxNode) -> bool {
    !node
        .ancestors()
        .any(|parent| ast::TokenTree::can_cast(parent.kind()))
}

#[tracing::instrument(level = "trace", skip(node))]
#[cfg(any(feature = "allows", feature = "cfg_scatter"))]
fn attributes(node: &SyntaxNode) -> impl Iterator<Item = ast::Attr> {
    ast::AnyHasAttrs::cast(node.clone())
        .map(|owner| ast::attrs_including_inner(&owner).collect::<Vec<_>>())
        .into_iter()
        .flatten()
}

/// Attribute containers are not always their policy owner (notably `mod`'s
/// inner attributes, whose policy context is the containing module).
#[cfg(any(feature = "allows", feature = "cfg_hygiene"))]
#[tracing::instrument(level = "trace", skip(attr))]
fn policy_owner(attr: &ast::Attr) -> Option<SyntaxNode> {
    let parent = attr.syntax().parent()?;
    if attr.kind().is_inner()
        && matches!(
            parent.kind(),
            ra_ap_syntax::SyntaxKind::ITEM_LIST | ra_ap_syntax::SyntaxKind::ASSOC_ITEM_LIST
        )
    {
        parent.parent()
    } else {
        Some(parent)
    }
}

#[tracing::instrument(level = "trace", skip(attr))]
#[cfg(feature = "allows")]
fn test_gate(attr: &ast::Attr) -> bool {
    match attr.meta() {
        Some(ast::Meta::CfgMeta(meta)) => meta
            .cfg_predicate()
            .is_some_and(|predicate| tokens::render(predicate.syntax()) == "test"),
        _ => false,
    }
}
