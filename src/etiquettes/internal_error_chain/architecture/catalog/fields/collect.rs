//! Lower storage syntax once, retaining the inputs needed by field policy.

use super::{Arguments, CopyRequirement, FieldType};
use crate::etiquettes::internal_error_chain::type_graph::type_label;
use syn::{GenericArgument, PathArguments, Type};
use tracing::instrument;

impl FieldType {
    #[instrument(level = "trace", skip(ty))]
    pub(crate) fn collect(ty: &Type) -> Self {
        Self {
            label: type_label(ty),
            static_str: is_static_str(ty),
            borrowed_string: is_borrowed_string(ty),
            copy: copy_requirement(ty),
        }
    }
}

#[instrument(level = "trace", skip(ty))]
fn is_static_str(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => {
            reference.mutability.is_none()
                && reference
                    .lifetime
                    .as_ref()
                    .is_some_and(|lifetime| lifetime.ident == "static")
                && matches!(&*reference.elem, Type::Path(path) if path.path.is_ident("str"))
        }
        Type::Paren(paren) => is_static_str(&paren.elem),
        Type::Group(group) => is_static_str(&group.elem),
        _ => false,
    }
}

#[instrument(level = "trace", skip(ty))]
fn is_borrowed_string(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => {
            matches!(&*reference.elem, Type::Path(path) if path.path.segments.last().is_some_and(|seg| seg.ident == "String"))
                || is_borrowed_string(&reference.elem)
        }
        Type::Paren(paren) => is_borrowed_string(&paren.elem),
        Type::Group(group) => is_borrowed_string(&group.elem),
        Type::Array(array) => is_borrowed_string(&array.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(is_borrowed_string),
        Type::Path(path) => path.path.segments.iter().any(|segment| {
            let PathArguments::AngleBracketed(args) = &segment.arguments else {
                return false;
            };
            args.args
                .iter()
                .any(|arg| matches!(arg, GenericArgument::Type(inner) if is_borrowed_string(inner)))
        }),
        _ => false,
    }
}

#[instrument(level = "trace", skip(ty))]
fn copy_requirement(ty: &Type) -> CopyRequirement {
    match ty {
        Type::Reference(_) | Type::Ptr(_) | Type::Never(_) => CopyRequirement::Always,
        Type::Paren(paren) => copy_requirement(&paren.elem),
        Type::Group(group) => copy_requirement(&group.elem),
        Type::Tuple(tuple) => {
            CopyRequirement::All(tuple.elems.iter().map(copy_requirement).collect())
        }
        Type::Array(array) => copy_requirement(&array.elem),
        Type::Path(path) => {
            let Some(segment) = path.path.segments.last() else {
                return CopyRequirement::Never;
            };
            CopyRequirement::Path {
                name: segment.ident.to_string(),
                arguments: match &segment.arguments {
                    PathArguments::None => Arguments::None,
                    PathArguments::AngleBracketed(args) => Arguments::Angle(
                        args.args
                            .iter()
                            .map(|arg| match arg {
                                GenericArgument::Type(inner) => copy_requirement(inner),
                                GenericArgument::Lifetime(_) | GenericArgument::Const(_) => {
                                    CopyRequirement::Always
                                }
                                _ => CopyRequirement::Never,
                            })
                            .collect(),
                    ),
                    _ => Arguments::Unsupported,
                },
            }
        }
        _ => CopyRequirement::Never,
    }
}
