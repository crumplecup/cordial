//! Collect signature facts from the database-issued canonical tree.

use super::syntax::scoped;
use super::{Function, ManualSource, SignatureChainIndex, Ty};
use crate::loader::{DatabaseSource, module_path_from_src_file};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericParams, HasModuleItem, HasName},
};
use std::{collections::BTreeMap, path::Path};

mod declarations;
mod expressions;
mod shapes;
mod sources;

impl SignatureChainIndex {
    #[tracing::instrument(level = "debug", skip(self, file))]
    #[hotpath::measure]
    pub(crate) fn collect(&mut self, file: &DatabaseSource<'_>, tree: &Path) {
        let scope = module_path_from_src_file(tree, file.path());
        self.native_items(file.tree().items(), &scope, file);
    }

    #[tracing::instrument(level = "trace", skip(self, items, file))]
    fn native_items(
        &mut self,
        items: impl Iterator<Item = ast::Item>,
        scope: &[String],
        file: &DatabaseSource<'_>,
    ) {
        for item in items {
            match item {
                ast::Item::Module(module) if !crate::ir::excluded_module(module.syntax()) => {
                    if let Some(items) = module.item_list()
                        && let Some(name) = module.name()
                    {
                        let mut child = scope.to_vec();
                        child.push(name.text().to_string());
                        self.native_items(items.items(), &child, file);
                    }
                }
                ast::Item::Use(item) => self.native_use(&item, scope),
                ast::Item::Fn(function) => {
                    self.native_function(&function, scope, None);
                    self.native_calls(&function, scope, file);
                }
                ast::Item::Impl(item) => self.native_impl(&item, scope, file),
                ast::Item::Struct(item) => self.native_struct(&item, scope),
                ast::Item::Enum(item) => self.native_enum(&item, scope),
                _ => {}
            }
        }
    }

    #[tracing::instrument(level = "trace", skip(self, item, file))]
    fn native_impl(&mut self, item: &ast::Impl, scope: &[String], file: &DatabaseSource<'_>) {
        let self_ty = item.self_ty().and_then(|ty| {
            let lowered = shapes::ty(&ty);
            self.resolve_type(&lowered, scope, &BTreeMap::new(), None)
                .or_else(|| match lowered {
                    super::facts::Type::Path(path) if path.parts().len() == 1 => Some(Ty {
                        path: scoped(scope, &path.parts()[0]),
                        args: Vec::new(),
                    }),
                    _ => None,
                })
        });
        let methods: Vec<_> = item
            .assoc_item_list()
            .into_iter()
            .flat_map(|list| list.assoc_items())
            .filter_map(|member| match member {
                ast::AssocItem::Fn(method) => Some(method),
                _ => None,
            })
            .collect();
        if let Some(ast::Type::PathType(trait_ty)) = item.trait_()
            && let Some(path) = trait_ty.path()
            && self
                .resolve_path(&shapes::path(&path), scope, None)
                .is_some_and(|path| {
                    matches!(path.as_str(), "std::error::Error" | "core::error::Error")
                })
            && let Some(self_ty) = &self_ty
        {
            let relationship = methods
                .iter()
                .find(|method| method.name().is_some_and(|name| name.text() == "source"))
                .map(sources::relationship)
                .unwrap_or(ManualSource::Default);
            self.manual_impls
                .entry(self_ty.path.clone())
                .or_default()
                .push(relationship);
        }
        for method in methods {
            self.native_function(&method, scope, self_ty.clone());
            self.native_calls(&method, scope, file);
        }
    }

    #[tracing::instrument(level = "trace", skip(self, function, self_ty))]
    fn native_function(&mut self, function: &ast::Fn, scope: &[String], self_ty: Option<Ty>) {
        let Some(output) = function.ret_type().and_then(|ret| ret.ty()) else {
            return;
        };
        let Some(name) = function.name() else {
            return;
        };
        let path = match &self_ty {
            Some(ty) => format!("{}::{}", ty.path, name.text()),
            None => scoped(scope, name.text()),
        };
        let params = function
            .param_list()
            .into_iter()
            .flat_map(|list| list.params())
            .filter_map(|param| param.ty())
            .map(|ty| shapes::ty(&ty))
            .collect();
        self.functions.entry(path).or_default().push(Function {
            scope: scope.to_vec(),
            params,
            output: shapes::ty(&output),
            generics: parameters(function).into_iter().collect(),
            self_ty,
        });
    }
}

#[tracing::instrument(level = "trace", skip(owner))]
fn parameters(owner: &impl HasGenericParams) -> Vec<String> {
    owner
        .generic_param_list()
        .into_iter()
        .flat_map(|list| list.generic_params())
        .filter_map(|param| match param {
            ast::GenericParam::TypeParam(ty) => ty.name().map(|name| name.text().to_string()),
            _ => None,
        })
        .collect()
}
