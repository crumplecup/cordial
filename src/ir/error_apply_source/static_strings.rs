//! Immutable initializer sources retained with their source snapshot.

use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasGenericArgs, HasModuleItem, HasName},
};
use std::collections::{BTreeMap, BTreeSet};

#[tracing::instrument(level = "debug", skip(file))]
pub(super) fn collect_native(file: &DatabaseSource<'_>) -> BTreeSet<String> {
    let mut declarations: BTreeMap<String, Vec<bool>> = BTreeMap::new();
    for item in file.tree().items() {
        let (name, ty, immutable, attrs_empty) = match item {
            ast::Item::Const(item) => (
                item.name(),
                item.ty(),
                true,
                item.attrs().next().is_none() && item.doc_comments().next().is_none(),
            ),
            ast::Item::Static(item) => (
                item.name(),
                item.ty(),
                item.mut_token().is_none(),
                item.attrs().next().is_none() && item.doc_comments().next().is_none(),
            ),
            _ => continue,
        };
        if let Some(name) = name {
            declarations
                .entry(name.text().to_string())
                .or_default()
                .push(immutable && attrs_empty && ty.as_ref().is_some_and(is_static_str));
        }
    }
    unique(declarations)
}

#[tracing::instrument(level = "trace", skip(ty))]
fn is_static_str(ty: &ast::Type) -> bool {
    let ast::Type::RefType(reference) = ty else {
        return false;
    };
    let Some(ast::Type::PathType(path_type)) = reference.ty() else {
        return false;
    };
    let Some(path) = path_type.path() else {
        return false;
    };
    reference.mut_token().is_none()
        && reference
            .lifetime()
            .is_some_and(|lifetime| lifetime.syntax().text() == "'static")
        && path.qualifier().is_none()
        && path.coloncolon_token().is_none()
        && path.segment().is_some_and(|segment| {
            segment.name_ref().is_some_and(|name| name.text() == "str")
                && segment.type_anchor().is_none()
                && segment.generic_arg_list().is_none()
                && segment.parenthesized_arg_list().is_none()
                && segment.coloncolon_token().is_none()
        })
}

#[tracing::instrument(level = "trace", skip(declarations))]
fn unique(declarations: BTreeMap<String, Vec<bool>>) -> BTreeSet<String> {
    declarations
        .into_iter()
        .filter_map(|(name, declarations)| (declarations.as_slice() == [true]).then_some(name))
        .collect()
}
