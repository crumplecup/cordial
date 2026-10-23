use std::collections::BTreeMap;
use std::path::Path;

use syn::{Fields, FnArg, Item, ItemImpl, ItemUse, ReturnType, Signature, Type, UseTree};

use super::syntax::{
    scoped, source_fields, source_returns_named_field, source_returns_variants, type_params,
};
use super::{ErrorType, Function, ManualSource, SignatureChainIndex, Ty};
use crate::etiquettes::internal_error_chain::type_graph::item_derives_error;
use crate::loader::module_path_from_src_file;

impl SignatureChainIndex {
    #[tracing::instrument(level = "debug", skip(self, syntax, file))]
    pub(crate) fn collect(&mut self, syntax: &syn::File, file: &Path, src_root: &Path) {
        let scope = module_path_from_src_file(src_root, file);
        self.collect_items(&syntax.items, &scope, file);
    }

    #[tracing::instrument(level = "debug", skip(self, items, file))]
    fn collect_items(&mut self, items: &[Item], scope: &[String], file: &Path) {
        for item in items {
            match item {
                Item::Mod(module) if !crate::enricher::is_cfg_test(&module.attrs) => {
                    if let Some((_, nested)) = &module.content {
                        let mut child = scope.to_vec();
                        child.push(module.ident.to_string());
                        self.collect_items(nested, &child, file);
                    }
                }
                Item::Use(item_use) => self.collect_use(item_use, scope),
                Item::Fn(function) => {
                    self.add_function(&function.sig, scope, None);
                    self.collect_calls(&function.block, &function.sig, scope, file);
                }
                Item::Impl(item_impl) => self.collect_impl(item_impl, scope, file),
                Item::Struct(item_struct) => {
                    let path = scoped(scope, &item_struct.ident.to_string());
                    let sources = source_fields(&item_struct.fields);
                    self.errors.entry(path).or_default().push(ErrorType {
                        scope: scope.to_vec(),
                        params: type_params(&item_struct.generics),
                        sources,
                        variant_sources: BTreeMap::new(),
                        error_impl: item_derives_error(&item_struct.attrs),
                    });
                }
                Item::Enum(item_enum) => {
                    let path = scoped(scope, &item_enum.ident.to_string());
                    let sources = item_enum
                        .variants
                        .iter()
                        .flat_map(|v| source_fields(&v.fields))
                        .collect();
                    let variant_sources = item_enum
                        .variants
                        .iter()
                        .filter_map(|variant| {
                            let ty = match &variant.fields {
                                Fields::Named(fields) => fields
                                    .named
                                    .iter()
                                    .find(|field| {
                                        field.ident.as_ref().is_some_and(|name| name == "source")
                                    })
                                    .map(|field| field.ty.clone()),
                                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                                    fields.unnamed.first().map(|field| field.ty.clone())
                                }
                                _ => None,
                            }?;
                            Some((variant.ident.to_string(), ty))
                        })
                        .collect();
                    self.errors.entry(path).or_default().push(ErrorType {
                        scope: scope.to_vec(),
                        params: type_params(&item_enum.generics),
                        sources,
                        variant_sources,
                        error_impl: item_derives_error(&item_enum.attrs),
                    });
                }
                _ => {}
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self, item, file))]
    fn collect_impl(&mut self, item: &ItemImpl, scope: &[String], file: &Path) {
        let self_ty = self
            .resolve_type(&item.self_ty, scope, &BTreeMap::new(), None)
            .or_else(|| match item.self_ty.as_ref() {
                Type::Path(path) if path.path.segments.len() == 1 => Some(Ty {
                    path: scoped(scope, &path.path.segments[0].ident.to_string()),
                    args: Vec::new(),
                }),
                _ => None,
            });
        if let Some((trait_path, _)) = &item.trait_
            && self
                .resolve_path(trait_path, scope, None)
                .is_some_and(|path| {
                    matches!(path.as_str(), "std::error::Error" | "core::error::Error")
                })
            && let Some(self_ty) = &self_ty
        {
            let source_method = item.items.iter().find_map(|member| match member {
                syn::ImplItem::Fn(method) if method.sig.ident == "source" => Some(method),
                _ => None,
            });
            let relationship = match source_method {
                None => ManualSource::Default,
                Some(method) if source_returns_named_field(method) => ManualSource::NamedField,
                Some(method) => source_returns_variants(method)
                    .map(ManualSource::Variants)
                    .unwrap_or(ManualSource::Opaque),
            };
            self.manual_impls
                .entry(self_ty.path.clone())
                .or_default()
                .push(relationship);
        }
        for member in &item.items {
            if let syn::ImplItem::Fn(method) = member {
                self.add_function(&method.sig, scope, self_ty.clone());
                self.collect_calls(&method.block, &method.sig, scope, file);
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self, sig, self_ty))]
    fn add_function(&mut self, sig: &Signature, scope: &[String], self_ty: Option<Ty>) {
        let mut path = scope.join("::");
        if let Some(ty) = &self_ty {
            path = ty.path.clone();
        }
        let path = if path.is_empty() {
            sig.ident.to_string()
        } else {
            format!("{path}::{}", sig.ident)
        };
        let ReturnType::Type(_, output) = &sig.output else {
            return;
        };
        let params = sig
            .inputs
            .iter()
            .filter_map(|arg| match arg {
                FnArg::Typed(typed) => Some((*typed.ty).clone()),
                FnArg::Receiver(_) => None,
            })
            .collect();
        self.functions
            .entry(path.clone())
            .or_default()
            .push(Function {
                scope: scope.to_vec(),
                params,
                output: (**output).clone(),
                generics: type_params(&sig.generics).into_iter().collect(),
                self_ty,
            });
    }

    #[tracing::instrument(level = "debug", skip(self, item))]
    fn collect_use(&mut self, item: &ItemUse, scope: &[String]) {
        fn walk(tree: &UseTree, prefix: Vec<String>, out: &mut Vec<(String, String)>) {
            match tree {
                UseTree::Path(path) => {
                    let mut next = prefix;
                    next.push(path.ident.to_string());
                    walk(&path.tree, next, out);
                }
                UseTree::Name(name) => {
                    let mut full = prefix;
                    full.push(name.ident.to_string());
                    out.push((name.ident.to_string(), full.join("::")));
                }
                UseTree::Rename(rename) => {
                    let mut full = prefix;
                    full.push(rename.ident.to_string());
                    out.push((rename.rename.to_string(), full.join("::")));
                }
                UseTree::Group(group) => {
                    for child in &group.items {
                        walk(child, prefix.clone(), out);
                    }
                }
                UseTree::Glob(_) => {}
            }
        }
        let mut imports = Vec::new();
        walk(&item.tree, Vec::new(), &mut imports);
        self.imports
            .entry(scope.to_vec())
            .or_default()
            .extend(imports);
    }
}
