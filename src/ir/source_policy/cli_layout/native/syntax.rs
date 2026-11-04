//! Canonical syntax helpers for native CLI collection.
use super::super::catalog::VariantShape;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericArgs, HasName},
};
use std::collections::BTreeMap;
#[tracing::instrument(level = "trace", skip(path))]
pub(super) fn path_names(path: &ast::Path) -> Vec<String> {
    let mut out = path.qualifier().map(|p| path_names(&p)).unwrap_or_default();
    if let Some(segment) = path.segment() {
        out.push(
            segment
                .name_ref()
                .map(|n| n.text().to_string())
                .unwrap_or_else(|| segment.syntax().text().to_string()),
        );
    }
    out
}
#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn type_names(ty: &ast::Type) -> Vec<String> {
    match ty {
        ast::Type::PathType(p) => {
            let Some(path) = p.path() else {
                return Vec::new();
            };
            let mut out = path_names(&path)
                .last()
                .cloned()
                .into_iter()
                .collect::<Vec<_>>();
            let mut paths = Vec::new();
            let mut current = Some(path);
            while let Some(path) = current {
                current = path.qualifier();
                paths.push(path);
            }
            for path in paths.into_iter().rev() {
                if let Some(args) = path.segment().and_then(|s| s.generic_arg_list()) {
                    for arg in args.generic_args() {
                        if let ast::GenericArg::TypeArg(arg) = arg
                            && let Some(ty) = arg.ty()
                        {
                            out.extend(type_names(&ty));
                        }
                    }
                }
            }
            out
        }
        ast::Type::RefType(t) => t.ty().map(|t| type_names(&t)).unwrap_or_default(),
        ast::Type::ParenType(t) => t.ty().map(|t| type_names(&t)).unwrap_or_default(),
        ast::Type::TupleType(t) => t.fields().flat_map(|t| type_names(&t)).collect(),
        ast::Type::SliceType(t) => t.ty().map(|t| type_names(&t)).unwrap_or_default(),
        ast::Type::ArrayType(t) => t.ty().map(|t| type_names(&t)).unwrap_or_default(),
        ast::Type::PtrType(t) => t.ty().map(|t| type_names(&t)).unwrap_or_default(),
        _ => Vec::new(),
    }
}
#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn type_label(ty: &ast::Type) -> String {
    match ty {
        ast::Type::PathType(t) => t
            .path()
            .map(|p| path_names(&p).join("::"))
            .unwrap_or_else(|| "?".into()),
        ast::Type::RefType(t) => t.ty().map(|t| type_label(&t)).unwrap_or_else(|| "?".into()),
        ast::Type::ParenType(t) => t.ty().map(|t| type_label(&t)).unwrap_or_else(|| "?".into()),
        _ => "?".into(),
    }
}
#[tracing::instrument(level = "trace", skip(fields))]
pub(super) fn fields(fields: Option<ast::FieldList>) -> BTreeMap<String, Vec<String>> {
    match fields {
        Some(ast::FieldList::RecordFieldList(list)) => list
            .fields()
            .filter_map(|f| Some((f.name()?.text().to_string(), type_names(&f.ty()?))))
            .collect(),
        Some(ast::FieldList::TupleFieldList(list)) => list
            .fields()
            .enumerate()
            .filter_map(|(i, f)| Some((i.to_string(), type_names(&f.ty()?))))
            .collect(),
        None => BTreeMap::new(),
    }
}
#[tracing::instrument(level = "trace", skip(list))]
pub(super) fn variant(list: Option<ast::FieldList>) -> VariantShape {
    match list {
        Some(ast::FieldList::RecordFieldList(list)) => {
            VariantShape::Named(fields(Some(ast::FieldList::RecordFieldList(list))))
        }
        Some(ast::FieldList::TupleFieldList(list)) => VariantShape::Unnamed(
            list.fields()
                .filter_map(|f| f.ty())
                .map(|t| type_names(&t))
                .collect(),
        ),
        None => VariantShape::Unit,
    }
}
