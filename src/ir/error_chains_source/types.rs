//! Type-shaped chain evidence; labels preserve the existing policy's grammar.

use super::Fact;
use crate::etiquettes::error_chain::ErrorChainProbeId;
use crate::ir::error_sites_source::native::path_label;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasGenericArgs, HasName},
};

#[tracing::instrument(level = "trace", skip(node))]
pub(super) fn collect(node: &SyntaxNode) -> Vec<Fact> {
    if let Some(item) = ast::Struct::cast(node.clone()) {
        let Some(ast::FieldList::RecordFieldList(fields)) = item.field_list() else {
            return Vec::new();
        };
        return fields
            .fields()
            .filter(|field| field.name().is_some_and(|name| name.text() == "source"))
            .filter_map(|field| {
                let ty = field.ty()?;
                let name = item.name()?;
                (!is_string(&ty)).then(|| Fact {
                    rule: ErrorChainProbeId::WrapperSourceField001,
                    node: field.syntax().clone(),
                    snippet: format!("struct {} {{ source: {} }}", name.text(), type_label(&ty)),
                    foreign: foreign_type(&ty),
                })
            })
            .collect();
    }
    if let Some(item) = ast::Enum::cast(node.clone()) {
        let Some(name) = item.name() else {
            return Vec::new();
        };
        if !name.text().ends_with("Kind") {
            return Vec::new();
        }
        return item
            .variant_list()
            .into_iter()
            .flat_map(|list| list.variants())
            .filter_map(|variant| {
                let ty = single_payload(variant.field_list()?)?;
                if is_string(&ty) {
                    return None;
                }
                Some(Fact {
                    rule: ErrorChainProbeId::KindWrapperPayload001,
                    node: variant.syntax().clone(),
                    snippet: format!(
                        "enum {} {{ {}({}) }}",
                        name.text(),
                        variant.name()?.text(),
                        type_label(&ty)
                    ),
                    foreign: foreign_type(&ty),
                })
            })
            .collect();
    }
    let Some(item) = ast::Impl::cast(node.clone()) else {
        return Vec::new();
    };
    let Some(ty) = from_source(&item) else {
        return Vec::new();
    };
    if is_string(&ty) || !is_foreign(&ty) {
        return Vec::new();
    }
    let Some(target) = item.self_ty() else {
        return Vec::new();
    };
    vec![Fact {
        rule: ErrorChainProbeId::FromBridge001,
        node: node.clone(),
        snippet: format!("impl From<{}> for {}", type_label(&ty), type_label(&target)),
        foreign: foreign_type(&ty),
    }]
}

#[tracing::instrument(level = "trace", skip(fields))]
fn single_payload(fields: ast::FieldList) -> Option<ast::Type> {
    let types: Vec<_> = match fields {
        ast::FieldList::RecordFieldList(list) => list.fields().map(|field| field.ty()).collect(),
        ast::FieldList::TupleFieldList(list) => list.fields().map(|field| field.ty()).collect(),
    };
    if types.len() == 1 {
        types.into_iter().next().flatten()
    } else {
        None
    }
}

#[tracing::instrument(level = "trace", skip(item))]
fn from_source(item: &ast::Impl) -> Option<ast::Type> {
    let ast::Type::PathType(ty) = item.trait_()? else {
        return None;
    };
    let segment = ty.path()?.segment()?;
    if segment.name_ref()?.text() != "From" {
        return None;
    }
    let ast::GenericArg::TypeArg(arg) = segment.generic_arg_list()?.generic_args().next()? else {
        return None;
    };
    arg.ty()
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn type_label(ty: &ast::Type) -> String {
    match ty {
        ast::Type::PathType(ty) => ty
            .path()
            .map(|path| path_label(&path))
            .unwrap_or_else(|| "?".into()),
        ast::Type::RefType(ty) => ty
            .ty()
            .map(|ty| type_label(&ty))
            .unwrap_or_else(|| "?".into()),
        ast::Type::ParenType(ty) => ty
            .ty()
            .map(|ty| type_label(&ty))
            .unwrap_or_else(|| "?".into()),
        _ => "?".into(),
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
fn is_string(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::PathType(ty) => ty.path().is_some_and(|path| {
            path.qualifier().is_none()
                && path.segment().is_some_and(|segment| {
                    segment.coloncolon_token().is_none()
                        && segment.generic_arg_list().is_none()
                        && segment
                            .name_ref()
                            .is_some_and(|name| name.text() == "String")
                })
        }),
        ast::Type::RefType(ty) => ty.ty().is_some_and(|ty| is_string(&ty)),
        ast::Type::ParenType(ty) => ty.ty().is_some_and(|ty| is_string(&ty)),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
fn is_foreign(ty: &ast::Type) -> bool {
    let label = type_label(ty);
    [
        "std::",
        "serde_json::",
        "serde_yaml::",
        "syn::",
        "csv::",
        "cargo_metadata::",
        "reqwest::",
        "url::",
        "toml::",
        "walkdir::",
        "tempfile::",
    ]
    .iter()
    .any(|prefix| label.starts_with(prefix))
        || label.ends_with("Error") && label.contains("::") && !label.ends_with("ErrorKind")
}

#[tracing::instrument(level = "trace", skip(ty))]
fn foreign_type(ty: &ast::Type) -> Option<String> {
    let label = type_label(ty);
    (label != "String" && label != "?").then_some(label)
}
