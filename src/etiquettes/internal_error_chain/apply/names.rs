//! Conservative name checks for syntactic error field rewrites.

use std::collections::BTreeSet;
use syn::visit::Visit;
use syn::{File, Item, ItemType, ItemUse, Type, UseTree};
use tracing::instrument;

/// Decline a rewrite if a name it relies on can mean something else locally.
/// This does not replace Rust name resolution; it keeps the narrow supported
/// forms from silently accepting a visible shadow or constructor alias.
#[instrument(level = "debug", skip(syntax))]
pub(super) fn audit_names(syntax: &File) -> NameAudit {
    let mut visitor = NameVisitor::default();
    visitor.visit_file(syntax);
    visitor.audit
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct NameAudit {
    ambiguous: bool,
    aliased_types: BTreeSet<String>,
    imported_names: BTreeSet<String>,
}

impl NameAudit {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn blocks(&self, type_name: &str) -> bool {
        self.ambiguous
            || self.aliased_types.contains(type_name)
            || self.imported_names.contains(type_name)
    }
}

#[derive(Default)]
struct NameVisitor {
    audit: NameAudit,
}

impl<'ast> Visit<'ast> for NameVisitor {
    #[instrument(level = "debug", skip(self, parameter))]
    fn visit_generic_param(&mut self, parameter: &'ast syn::GenericParam) {
        if matches!(parameter, syn::GenericParam::Type(ty) if protected_name(&ty.ident.to_string()))
        {
            self.audit.ambiguous = true;
        }
        syn::visit::visit_generic_param(self, parameter);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item(&mut self, item: &'ast Item) {
        let name = match item {
            Item::Struct(item) => Some(&item.ident),
            Item::Enum(item) => Some(&item.ident),
            Item::Union(item) => Some(&item.ident),
            Item::Trait(item) => Some(&item.ident),
            Item::Mod(item) => Some(&item.ident),
            Item::Const(item) => Some(&item.ident),
            Item::Static(item) => Some(&item.ident),
            Item::Fn(item) => Some(&item.sig.ident),
            Item::Macro(item) => item.ident.as_ref(),
            Item::ExternCrate(item) => Some(
                item.rename
                    .as_ref()
                    .map(|(_, name)| name)
                    .unwrap_or(&item.ident),
            ),
            _ => None,
        };
        if name.is_some_and(|name| {
            matches!(
                name.to_string().as_str(),
                "String" | "Location" | "str" | "std" | "core" | "alloc" | "write" | "writeln"
            )
        }) {
            self.audit.ambiguous = true;
        }
        syn::visit::visit_item(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_type(&mut self, item: &'ast ItemType) {
        if protected_name(&item.ident.to_string()) {
            self.audit.ambiguous = true;
        }
        if let Some(target) = type_name(&item.ty) {
            if item.ident != target {
                self.audit.aliased_types.insert(target);
            }
        }
        syn::visit::visit_item_type(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        self.inspect_use(&item.tree, &mut Vec::new());
        syn::visit::visit_item_use(self, item);
    }

    #[instrument(level = "debug", skip(self, _item))]
    fn visit_item_mod(&mut self, _item: &'ast syn::ItemMod) {
        // The candidate and its recognized constructors are at file scope.
    }
}

impl NameVisitor {
    #[instrument(level = "debug", skip(self, tree, path))]
    fn inspect_use(&mut self, tree: &UseTree, path: &mut Vec<String>) {
        match tree {
            UseTree::Path(part) => {
                path.push(part.ident.to_string());
                self.inspect_use(&part.tree, path);
                path.pop();
            }
            UseTree::Name(part) => {
                path.push(part.ident.to_string());
                self.check_binding(&part.ident.to_string(), path);
                path.pop();
            }
            UseTree::Rename(part) => {
                path.push(part.ident.to_string());
                self.check_binding(&part.rename.to_string(), path);
                path.pop();
            }
            UseTree::Group(group) => {
                for item in &group.items {
                    self.inspect_use(item, path);
                }
            }
            UseTree::Glob(_) => self.audit.ambiguous = true,
        }
    }

    #[instrument(level = "debug", skip(self, path))]
    fn check_binding(&mut self, binding: &str, path: &[String]) {
        self.audit.imported_names.insert(binding.to_string());
        if let Some(target) = path.last() {
            if target != binding {
                self.audit.aliased_types.insert(target.clone());
            }
        }
        if !protected_name(binding) {
            return;
        }
        let known = matches!(path, [root, module, name]
            if (root == "std" || root == "alloc") && module == "string" && name == "String" && binding == "String")
            || matches!(path, [root, module, name]
                if (root == "std" || root == "core") && module == "panic" && name == "Location" && binding == "Location");
        if !known {
            self.audit.ambiguous = true;
        }
    }
}

#[instrument(level = "debug")]
fn protected_name(name: &str) -> bool {
    matches!(
        name,
        "String" | "Location" | "str" | "std" | "core" | "alloc" | "write" | "writeln"
    )
}

#[instrument(level = "debug", skip(ty))]
fn type_name(ty: &Type) -> Option<String> {
    let Type::Path(path) = ty else { return None };
    (path.qself.is_none())
        .then(|| {
            path.path
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
        })
        .flatten()
}
