//! Which parameter and return types tracing can record without extra bounds.

use std::collections::HashSet;

use syn::{FnArg, GenericArgument, Pat, PathArguments, ReturnType, Signature, Type, TypePath};

use tracing::instrument;

#[instrument(level = "debug", skip(sig))]
pub(super) fn unrecordable_params(sig: &Signature) -> Vec<String> {
    let generics = type_param_names(sig);
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pat) => {
                let Pat::Ident(ident) = &*pat.pat else {
                    return None;
                };
                if type_is_unrecordable(&pat.ty) || type_is_generic_param(&pat.ty, &generics) {
                    Some(ident.ident.to_string())
                } else {
                    None
                }
            }
            FnArg::Receiver(_) => None,
        })
        .collect()
}

#[instrument(level = "debug", skip(sig))]
fn type_param_names(sig: &Signature) -> HashSet<String> {
    sig.generics
        .type_params()
        .map(|param| param.ident.to_string())
        .collect()
}

#[instrument(level = "debug", skip(sig))]
pub(super) fn return_type_unrecordable(sig: &Signature) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => type_is_unrecordable(ty),
        ReturnType::Default => false,
    }
}

#[instrument(level = "debug", skip(ty))]
fn type_is_unrecordable(ty: &Type) -> bool {
    match ty {
        Type::ImplTrait(_) | Type::TraitObject(_) | Type::BareFn(_) | Type::Infer(_) => true,
        Type::Never(_) | Type::Macro(_) | Type::Verbatim(_) => true,
        Type::Reference(reference) => type_is_unrecordable(&reference.elem),
        Type::Ptr(ptr) => type_is_unrecordable(&ptr.elem),
        Type::Paren(paren) => type_is_unrecordable(&paren.elem),
        Type::Group(group) => type_is_unrecordable(&group.elem),
        Type::Slice(slice) => type_is_unrecordable(&slice.elem),
        Type::Array(array) => type_is_unrecordable(&array.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(type_is_unrecordable),
        Type::Path(path) => path_is_unrecordable(path),
        _ => true,
    }
}

/// Types tracing can record without a `Debug` bound blowing up the build.
const RECORDABLE_IDENTS: &[&str] = &[
    "bool", "char", "str", "String", "OsStr", "OsString", "Path", "PathBuf", "u8", "u16", "u32",
    "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "f32", "f64", "Duration",
    "Option", "Result", "Vec", "Box", "Arc", "Rc", "Cow",
];

#[instrument(level = "debug", skip(path))]
fn path_is_unrecordable(path: &TypePath) -> bool {
    let Some(last) = path.path.segments.last() else {
        return true;
    };
    if !RECORDABLE_IDENTS.contains(&last.ident.to_string().as_str()) {
        return true;
    }
    match &last.arguments {
        PathArguments::None => false,
        PathArguments::Parenthesized(_) => true,
        PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| match arg {
            GenericArgument::Type(inner) => type_is_unrecordable(inner),
            GenericArgument::AssocType(assoc) => type_is_unrecordable(&assoc.ty),
            _ => false,
        }),
    }
}

#[instrument(level = "debug", skip(sig))]
pub(super) fn return_type_borrowed(sig: &Signature) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => type_contains_borrow(ty),
        ReturnType::Default => false,
    }
}

#[instrument(level = "debug", skip(ty))]
fn type_contains_borrow(ty: &Type) -> bool {
    match ty {
        Type::Reference(_) | Type::Ptr(_) => true,
        Type::Paren(paren) => type_contains_borrow(&paren.elem),
        Type::Group(group) => type_contains_borrow(&group.elem),
        Type::Slice(slice) => type_contains_borrow(&slice.elem),
        Type::Array(array) => type_contains_borrow(&array.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(type_contains_borrow),
        Type::Path(path) => path
            .path
            .segments
            .iter()
            .any(|segment| match &segment.arguments {
                PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| match arg {
                    GenericArgument::Type(inner) => type_contains_borrow(inner),
                    GenericArgument::AssocType(assoc) => type_contains_borrow(&assoc.ty),
                    _ => false,
                }),
                _ => false,
            }),
        _ => false,
    }
}

#[instrument(level = "debug", skip(ty, generics))]
fn type_is_generic_param(ty: &Type, generics: &HashSet<String>) -> bool {
    match ty {
        Type::Path(TypePath { qself: None, path }) => path.segments.iter().any(|segment| {
            generics.contains(&segment.ident.to_string())
                || match &segment.arguments {
                    PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| match arg {
                        GenericArgument::Type(inner) => type_is_generic_param(inner, generics),
                        _ => false,
                    }),
                    _ => false,
                }
        }),
        Type::Reference(reference) => type_is_generic_param(&reference.elem, generics),
        Type::Ptr(ptr) => type_is_generic_param(&ptr.elem, generics),
        Type::Paren(paren) => type_is_generic_param(&paren.elem, generics),
        Type::Group(group) => type_is_generic_param(&group.elem, generics),
        Type::Slice(slice) => type_is_generic_param(&slice.elem, generics),
        Type::Array(array) => type_is_generic_param(&array.elem, generics),
        Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .any(|inner| type_is_generic_param(inner, generics)),
        _ => false,
    }
}
