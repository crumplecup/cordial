//! Lower canonical type syntax to the retained, conservative signature grammar.

use super::super::facts::{Path, Type};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericArgs},
};

#[tracing::instrument(level = "trace", skip(value))]
pub(super) fn path(value: &ast::Path) -> Path {
    let mut parts = Vec::new();
    let mut current = Some(value.clone());
    let mut simple = true;
    while let Some(value) = current {
        if let Some(segment) = value.segment() {
            if let Some(name) = segment.name_ref() {
                parts.push(name.text().to_string());
            } else if let Some(anchor) = segment.type_anchor() {
                // TypeAnchor::path_type() selects the first path child, which
                // can be the self type. Only the path after `as` is a trait.
                if anchor.as_token().is_some()
                    && let Some(trait_path) = anchor
                        .syntax()
                        .children()
                        .filter_map(ast::PathType::cast)
                        .last()
                        .and_then(|ty| ty.path())
                {
                    parts.extend(path(&trait_path).parts().iter().rev().cloned());
                }
            }
            simple &= segment.coloncolon_token().is_none()
                && segment.generic_arg_list().is_none()
                && segment.parenthesized_arg_list().is_none();
        }
        simple &= value.coloncolon_token().is_none();
        current = value.qualifier();
    }
    parts.reverse();
    simple &= parts.len() == 1;
    let args = value.segment().and_then(|segment| {
        if segment.parenthesized_arg_list().is_some() {
            return None;
        }
        Some(
            segment
                .generic_arg_list()
                .into_iter()
                .flat_map(|list| list.generic_args())
                .filter_map(|arg| match arg {
                    ast::GenericArg::TypeArg(arg) => arg.ty().map(|value| ty(&value)),
                    _ => None,
                })
                .collect(),
        )
    });
    Path::new(parts, args, simple)
}

#[tracing::instrument(level = "trace", skip(value))]
pub(super) fn ty(value: &ast::Type) -> Type {
    match value {
        ast::Type::PathType(value) => match value.path() {
            Some(value) if !anchored(&value) => Type::Path(path(&value)),
            _ => Type::Unsupported,
        },
        ast::Type::RefType(value) => value
            .ty()
            .map(|value| Type::Transparent(Box::new(ty(&value))))
            .unwrap_or(Type::Unsupported),
        ast::Type::ParenType(value) => value
            .ty()
            .map(|value| Type::Transparent(Box::new(ty(&value))))
            .unwrap_or(Type::Unsupported),
        ast::Type::TupleType(value) => {
            Type::Tuple(value.fields().map(|value| ty(&value)).collect())
        }
        _ => Type::Unsupported,
    }
}

#[tracing::instrument(level = "trace", skip(value))]
fn anchored(value: &ast::Path) -> bool {
    value
        .segment()
        .is_some_and(|segment| segment.type_anchor().is_some())
        || value.qualifier().is_some_and(|value| anchored(&value))
}
