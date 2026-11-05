//! Declaration and import facts; collection order matches the source contract.

use super::super::{ErrorType, SignatureChainIndex, syntax::scoped};
use super::{parameters, shapes};
use ra_ap_syntax::ast::{self, HasAttrs, HasName};
use std::collections::BTreeMap;

impl SignatureChainIndex {
    #[tracing::instrument(level = "trace", skip(self, item))]
    pub(super) fn native_struct(&mut self, item: &ast::Struct, scope: &[String]) {
        let Some(name) = item.name() else {
            return;
        };
        self.errors
            .entry(scoped(scope, name.text()))
            .or_default()
            .push(ErrorType {
                scope: scope.to_vec(),
                params: parameters(item),
                sources: source_fields(item.field_list()),
                variant_sources: BTreeMap::new(),
                error_impl: crate::ir::derives_error(item),
            });
    }

    #[tracing::instrument(level = "trace", skip(self, item))]
    pub(super) fn native_enum(&mut self, item: &ast::Enum, scope: &[String]) {
        let Some(name) = item.name() else {
            return;
        };
        let mut sources = Vec::new();
        let mut variant_sources = BTreeMap::new();
        for variant in item
            .variant_list()
            .into_iter()
            .flat_map(|list| list.variants())
        {
            sources.extend(source_fields(variant.field_list()));
            let source = match variant.field_list() {
                Some(ast::FieldList::RecordFieldList(fields)) => fields
                    .fields()
                    .find(|field| field.name().is_some_and(|name| name.text() == "source"))
                    .and_then(|field| field.ty()),
                Some(ast::FieldList::TupleFieldList(fields)) if fields.fields().count() == 1 => {
                    fields.fields().next().and_then(|field| field.ty())
                }
                _ => None,
            };
            if let Some(source) = source
                && let Some(name) = variant.name()
            {
                variant_sources.insert(name.text().to_string(), shapes::ty(&source));
            }
        }
        self.errors
            .entry(scoped(scope, name.text()))
            .or_default()
            .push(ErrorType {
                scope: scope.to_vec(),
                params: parameters(item),
                sources,
                variant_sources,
                error_impl: crate::ir::derives_error(item),
            });
    }

    #[tracing::instrument(level = "trace", skip(self, item))]
    pub(super) fn native_use(&mut self, item: &ast::Use, scope: &[String]) {
        if let Some(tree) = item.use_tree() {
            let mut imports = Vec::new();
            imports_from(&tree, Vec::new(), &mut imports);
            self.imports
                .entry(scope.join("::"))
                .or_default()
                .extend(imports);
        }
    }
}

#[tracing::instrument(level = "trace", skip(fields))]
fn source_fields(fields: Option<ast::FieldList>) -> Vec<super::super::facts::Type> {
    match fields {
        Some(ast::FieldList::RecordFieldList(fields)) => fields
            .fields()
            .filter(|field| {
                source_attribute(field) || field.name().is_some_and(|name| name.text() == "source")
            })
            .filter_map(|field| field.ty())
            .map(|ty| shapes::ty(&ty))
            .collect(),
        Some(ast::FieldList::TupleFieldList(fields)) => fields
            .fields()
            .filter(source_attribute)
            .filter_map(|field| field.ty())
            .map(|ty| shapes::ty(&ty))
            .collect(),
        _ => Vec::new(),
    }
}

#[tracing::instrument(level = "trace", skip(owner))]
fn source_attribute(owner: &impl HasAttrs) -> bool {
    owner
        .attrs()
        .any(|attr| matches!(attr.simple_name().as_deref(), Some("source" | "from")))
}

#[tracing::instrument(level = "trace", skip(tree, out))]
fn imports_from(tree: &ast::UseTree, mut prefix: Vec<String>, out: &mut Vec<(String, String)>) {
    if tree.star_token().is_some() {
        return;
    }
    if let Some(path) = tree.path() {
        prefix.extend(shapes::path(&path).parts().iter().cloned());
    }
    if let Some(group) = tree.use_tree_list() {
        for child in group.use_trees() {
            imports_from(&child, prefix.clone(), out);
        }
    } else if let Some(name) = tree
        .rename()
        .and_then(|rename| {
            rename
                .name()
                .map(|name| name.text().to_string())
                .or_else(|| rename.underscore_token().map(|_| "_".to_string()))
        })
        .or_else(|| prefix.last().cloned())
    {
        out.push((name, prefix.join("::")));
    }
}
