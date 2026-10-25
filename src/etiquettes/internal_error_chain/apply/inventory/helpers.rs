//! Name-use helpers for the conversion-site visitor.

use syn::visit::Visit;
use syn::{PathArguments, Type, TypePath, UseTree};
use tracing::instrument;

/// Only these familiar derives have no hidden constructor or alias path.
#[instrument(level = "debug", skip(attr))]
pub(super) fn derive_is_nonconstructing(attr: &syn::Attribute) -> bool {
    derive_matches(attr, true)
}

/// Restrict declaration links to compiler-provided derives.
#[instrument(level = "debug", skip(attr))]
pub(super) fn derive_is_builtin(attr: &syn::Attribute) -> bool {
    derive_matches(attr, false)
}

#[instrument(level = "trace", skip(attr))]
fn derive_matches(attr: &syn::Attribute, allow_error: bool) -> bool {
    if !attr.path().is_ident("derive") {
        return false;
    }
    let mut known = true;
    attr.parse_nested_meta(|meta| {
        known &= meta.path.segments.len() == 1
            && meta.path.segments.last().is_some_and(|part| {
                matches!(
                    part.ident.to_string().as_str(),
                    "Debug" | "Clone" | "Copy" | "PartialEq" | "Eq" | "Hash" | "PartialOrd" | "Ord"
                ) || (allow_error && part.ident == "Error")
            });
        Ok(())
    })
    .is_ok()
        && known
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn plain_type_path(ty: &Type) -> Option<Vec<String>> {
    let Type::Path(path) = ty else { return None };
    if path.qself.is_some()
        || path.path.segments.is_empty()
        || path.path.segments.iter().any(|part| {
            !matches!(&part.arguments, PathArguments::None)
                && !matches!(&part.arguments, PathArguments::AngleBracketed(arguments)
                    if arguments.args.iter().all(|argument| matches!(argument, syn::GenericArgument::Lifetime(_))))
        })
    {
        return None;
    }
    Some(
        path.path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect(),
    )
}

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
