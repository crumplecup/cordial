//! Name-use helpers for the conversion-site visitor.

use syn::visit::Visit;
use syn::{TypePath, UseTree};
use tracing::instrument;

#[instrument(level = "debug", skip(node))]
pub(super) fn type_mentions(node: &syn::Type, type_name: &str) -> bool {
    let mut visitor = TypeMentionVisitor {
        type_name,
        found: false,
    };
    visitor.visit_type(node);
    visitor.found
}

#[instrument(level = "debug", skip(node))]
pub(super) fn return_type_mentions(node: &syn::ReturnType, type_name: &str) -> bool {
    let mut visitor = TypeMentionVisitor {
        type_name,
        found: false,
    };
    visitor.visit_return_type(node);
    visitor.found
}

#[instrument(level = "debug")]
pub(super) fn path_mentions(node: &syn::Path, type_name: &str) -> bool {
    let mut visitor = TypeMentionVisitor {
        type_name,
        found: false,
    };
    visitor.visit_path(node);
    visitor.found
}

struct TypeMentionVisitor<'a> {
    type_name: &'a str,
    found: bool,
}

impl<'ast> Visit<'ast> for TypeMentionVisitor<'_> {
    #[instrument(level = "debug", skip(self, item))]
    fn visit_type_path(&mut self, item: &'ast TypePath) {
        self.found |= item
            .path
            .segments
            .iter()
            .any(|part| part.ident == self.type_name);
        syn::visit::visit_type_path(self, item);
    }
}

#[instrument(level = "debug", skip(tree))]
pub(super) fn use_tree_mentions(tree: &UseTree, name: &str) -> bool {
    match tree {
        UseTree::Path(part) => part.ident == name || use_tree_mentions(&part.tree, name),
        UseTree::Name(part) => part.ident == name,
        UseTree::Rename(part) => part.ident == name || part.rename == name,
        UseTree::Group(group) => group.items.iter().any(|item| use_tree_mentions(item, name)),
        UseTree::Glob(_) => true,
    }
}
