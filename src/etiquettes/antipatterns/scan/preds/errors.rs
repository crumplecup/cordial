//! Predicates and snippets for error-type antipatterns.

use syn::{PathArguments, Type, TypeParamBound, TypePath, TypeTraitObject};

use super::labels::{trait_bound_label, truncate_snippet, type_label};

use tracing::instrument;

#[instrument(level = "debug", skip(ty))]
pub(in crate::etiquettes::antipatterns::scan) fn result_error_type(ty: &Type) -> Option<&Type> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return None;
    };
    let segment = path.segments.last()?;
    if segment.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let type_args: Vec<&Type> = args
        .args
        .iter()
        .filter_map(|arg| match arg {
            syn::GenericArgument::Type(inner) => Some(inner),
            _ => None,
        })
        .collect();
    type_args.get(1).copied()
}

#[instrument(level = "trace", skip(ty))]
pub(in crate::etiquettes::antipatterns::scan) fn is_stringish_error_type(ty: &Type) -> bool {
    match ty {
        Type::Path(TypePath { path, .. }) => {
            let ident = path
                .segments
                .last()
                .map(|segment| segment.ident.to_string());
            matches!(ident.as_deref(), Some("String") | Some("str"))
        }
        Type::Reference(reference) => is_stringish_error_type(&reference.elem),
        Type::Paren(paren) => is_stringish_error_type(&paren.elem),
        Type::Group(group) => is_stringish_error_type(&group.elem),
        _ => false,
    }
}

#[instrument(level = "debug", skip(ty))]
pub(in crate::etiquettes::antipatterns::scan) fn result_string_error_snippet(ty: &Type) -> String {
    match ty {
        Type::Path(_) => format!(
            "Result<…, {}>",
            type_label(result_error_type(ty).unwrap_or(ty))
        ),
        Type::Paren(paren) => result_string_error_snippet(&paren.elem),
        Type::Group(group) => result_string_error_snippet(&group.elem),
        _ => "Result<…, String>".to_string(),
    }
}

#[instrument(level = "debug", skip(ty))]
pub(in crate::etiquettes::antipatterns::scan) fn box_dyn_error_trait_object(
    ty: &Type,
) -> Option<&TypeTraitObject> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return None;
    };
    let segment = path.segments.last()?;
    if segment.ident != "Box" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    let syn::GenericArgument::Type(inner) = &args.args[0] else {
        return None;
    };
    let Type::TraitObject(trait_obj) = inner else {
        return None;
    };
    if !trait_object_has_error_bound(trait_obj) {
        return None;
    }
    Some(trait_obj)
}

#[instrument(level = "debug", skip(trait_obj))]
fn trait_object_has_error_bound(trait_obj: &TypeTraitObject) -> bool {
    trait_obj.bounds.iter().any(|bound| match bound {
        TypeParamBound::Trait(trait_bound) => trait_bound
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Error"),
        _ => false,
    })
}

#[instrument(level = "debug", skip(trait_obj))]
pub(in crate::etiquettes::antipatterns::scan) fn box_dyn_error_snippet(
    trait_obj: &TypeTraitObject,
) -> String {
    let bounds: Vec<String> = trait_obj.bounds.iter().map(trait_bound_label).collect();
    let snippet = format!("Box<dyn {}>", bounds.join(" + "));
    truncate_snippet(&snippet, 96)
}
