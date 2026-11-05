//! Compatibility collection through the shared parsed-file carrier.

use super::{NameAudit, protected_name};
use syn::visit::Visit;
use syn::{Item, ItemType, ItemUse, Type, UseTree};
use tracing::instrument;

impl<'ast> Visit<'ast> for NameAudit {
    #[instrument(level = "debug", skip(self, parameter))]
    fn visit_generic_param(&mut self, parameter: &'ast syn::GenericParam) {
        if matches!(parameter, syn::GenericParam::Type(ty) if protected_name(&ty.ident.to_string()))
        {
            self.ambiguous = true;
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
            self.ambiguous = true;
        }
        syn::visit::visit_item(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_type(&mut self, item: &'ast ItemType) {
        if protected_name(&item.ident.to_string()) {
            self.ambiguous = true;
        }
        if let Some(target) = type_name(&item.ty) {
            if item.ident != target {
                self.aliased_types.insert(target);
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

impl NameAudit {
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
            UseTree::Glob(_) => self.ambiguous = true,
        }
    }
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
