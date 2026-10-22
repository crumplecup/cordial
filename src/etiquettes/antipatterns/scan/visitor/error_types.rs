//! Identify error types and the Kind enums they hold before field scanning.

use std::collections::HashSet;

use syn::visit::Visit;
use syn::{Attribute, Fields, ItemEnum, ItemImpl, ItemStruct, Type};
use tracing::instrument;

/// Identifiers of local types with an `Error` derive or implementation.
#[instrument(level = "debug", skip(file))]
pub(in crate::etiquettes::antipatterns::scan) fn collect_error_type_names(
    file: &syn::File,
) -> HashSet<String> {
    struct Collector(HashSet<String>);
    impl<'ast> Visit<'ast> for Collector {
        fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
            if derives_error(&node.attrs) {
                self.0.insert(node.ident.to_string());
            }
        }
        fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
            if derives_error(&node.attrs) {
                self.0.insert(node.ident.to_string());
            }
        }
        fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
            if node.trait_.as_ref().is_some_and(|(path, _)| {
                path.segments.last().is_some_and(|seg| seg.ident == "Error")
            }) && let Type::Path(path) = &*node.self_ty
                && let Some(seg) = path.path.segments.last()
            {
                self.0.insert(seg.ident.to_string());
            }
            syn::visit::visit_item_impl(self, node);
        }
    }
    let mut collector = Collector(HashSet::new());
    collector.visit_file(file);
    collector.0
}

/// Kind enums held by Error implementations participate in borrowed error storage.
#[instrument(level = "debug", skip(file, error_type_names))]
pub(in crate::etiquettes::antipatterns::scan) fn collect_error_kind_names(
    file: &syn::File,
    error_type_names: &HashSet<String>,
) -> HashSet<String> {
    struct Collector<'a> {
        error_types: &'a HashSet<String>,
        kinds: HashSet<String>,
    }
    impl<'ast> Visit<'ast> for Collector<'_> {
        fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
            if !self.error_types.contains(&node.ident.to_string()) {
                return;
            }
            let Fields::Named(fields) = &node.fields else {
                return;
            };
            for field in &fields.named {
                if field.ident.as_ref().is_none_or(|ident| ident != "kind") {
                    continue;
                }
                let ty = match &field.ty {
                    Type::Reference(reference) => &*reference.elem,
                    Type::Path(path)
                        if path
                            .path
                            .segments
                            .last()
                            .is_some_and(|seg| seg.ident == "Box") =>
                    {
                        let Some(seg) = path.path.segments.last() else {
                            continue;
                        };
                        let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
                            continue;
                        };
                        let Some(syn::GenericArgument::Type(ty)) = args.args.first() else {
                            continue;
                        };
                        ty
                    }
                    ty => ty,
                };
                if let Type::Path(path) = ty
                    && let Some(seg) = path.path.segments.last()
                    && seg.ident.to_string().ends_with("Kind")
                {
                    self.kinds.insert(seg.ident.to_string());
                }
            }
        }
    }
    let mut collector = Collector {
        error_types: error_type_names,
        kinds: HashSet::new(),
    };
    collector.visit_file(file);
    collector.kinds
}

#[instrument(level = "debug", skip(attrs))]
fn derives_error(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            found |= meta
                .path
                .segments
                .last()
                .is_some_and(|seg| seg.ident == "Error");
            Ok(())
        });
        found
    })
}
