//! Predicates and snippets for static reference fields.

use std::collections::HashSet;

use syn::{PathArguments, Type, TypeParamBound, TypeTraitObject};

use crate::config::StaticRefStrategy;

use super::labels::{truncate_snippet, type_label};

use tracing::instrument;

/// True when a field type contains a `&'static` that is not a crate-local `dyn Trait`.
#[instrument(level = "debug", skip(ty, local_trait_names), ret)]
pub(in crate::etiquettes::antipatterns::scan) fn type_contains_disallowed_static_ref(
    ty: &Type,
    local_trait_names: &HashSet<String>,
) -> bool {
    match ty {
        Type::Reference(reference) => {
            let is_static = reference
                .lifetime
                .as_ref()
                .is_some_and(|lifetime| lifetime.ident == "static");
            if is_static {
                if pointee_is_local_dyn_spine(&reference.elem, local_trait_names) {
                    type_contains_disallowed_static_ref(&reference.elem, local_trait_names)
                } else {
                    true
                }
            } else {
                type_contains_disallowed_static_ref(&reference.elem, local_trait_names)
            }
        }
        Type::Path(type_path) => type_path.path.segments.iter().any(|segment| {
            type_arguments_have_disallowed_static_ref(&segment.arguments, local_trait_names)
        }),
        Type::Array(array) => type_contains_disallowed_static_ref(&array.elem, local_trait_names),
        Type::Slice(slice) => type_contains_disallowed_static_ref(&slice.elem, local_trait_names),
        Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .any(|inner| type_contains_disallowed_static_ref(inner, local_trait_names)),
        Type::Paren(paren) => type_contains_disallowed_static_ref(&paren.elem, local_trait_names),
        Type::Group(group) => type_contains_disallowed_static_ref(&group.elem, local_trait_names),
        Type::Ptr(pointer) => type_contains_disallowed_static_ref(&pointer.elem, local_trait_names),
        Type::TraitObject(trait_obj) => {
            trait_object_has_disallowed_static_ref(trait_obj, local_trait_names)
        }
        _ => false,
    }
}

#[instrument(level = "trace", skip(arguments, local_trait_names), ret)]
fn type_arguments_have_disallowed_static_ref(
    arguments: &PathArguments,
    local_trait_names: &HashSet<String>,
) -> bool {
    match arguments {
        PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| {
            matches!(
                arg,
                syn::GenericArgument::Type(inner)
                    if type_contains_disallowed_static_ref(inner, local_trait_names)
            )
        }),
        PathArguments::Parenthesized(args) => args
            .inputs
            .iter()
            .any(|inner| type_contains_disallowed_static_ref(&inner.ty, local_trait_names)),
        PathArguments::None => false,
    }
}

#[instrument(level = "trace", skip(trait_obj, local_trait_names), ret)]
fn trait_object_has_disallowed_static_ref(
    trait_obj: &TypeTraitObject,
    local_trait_names: &HashSet<String>,
) -> bool {
    trait_obj.bounds.iter().any(|bound| match bound {
        TypeParamBound::Trait(trait_bound) => trait_bound.path.segments.iter().any(|segment| {
            type_arguments_have_disallowed_static_ref(&segment.arguments, local_trait_names)
        }),
        _ => false,
    })
}

/// `&'static dyn LocalTrait`, slices/arrays of that, and nested static refs to the same.
#[instrument(level = "trace", skip(ty, local_trait_names), ret)]
fn pointee_is_local_dyn_spine(ty: &Type, local_trait_names: &HashSet<String>) -> bool {
    match ty {
        Type::Paren(paren) => pointee_is_local_dyn_spine(&paren.elem, local_trait_names),
        Type::Group(group) => pointee_is_local_dyn_spine(&group.elem, local_trait_names),
        Type::TraitObject(trait_obj) => trait_object_is_local(trait_obj, local_trait_names),
        Type::Slice(slice) => pointee_is_local_dyn_spine(&slice.elem, local_trait_names),
        Type::Array(array) => pointee_is_local_dyn_spine(&array.elem, local_trait_names),
        Type::Reference(reference) => {
            reference
                .lifetime
                .as_ref()
                .is_some_and(|lifetime| lifetime.ident == "static")
                && pointee_is_local_dyn_spine(&reference.elem, local_trait_names)
        }
        _ => false,
    }
}

#[instrument(level = "trace", skip(trait_obj, local_trait_names), ret)]
fn trait_object_is_local(trait_obj: &TypeTraitObject, local_trait_names: &HashSet<String>) -> bool {
    trait_obj.bounds.iter().any(|bound| {
        let TypeParamBound::Trait(trait_bound) = bound else {
            return false;
        };
        trait_bound
            .path
            .segments
            .last()
            .is_some_and(|segment| local_trait_names.contains(&segment.ident.to_string()))
    })
}

#[instrument(level = "debug", skip(ty), ret)]
pub(in crate::etiquettes::antipatterns::scan) fn type_is_location_capture(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => type_is_location_capture(&reference.elem),
        Type::Paren(paren) => type_is_location_capture(&paren.elem),
        Type::Group(group) => type_is_location_capture(&group.elem),
        Type::Path(type_path) => type_path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Location"),
        _ => false,
    }
}

#[instrument(level = "debug", skip(ty, strategy))]
pub(in crate::etiquettes::antipatterns::scan) fn static_ref_field_snippet(
    ty: &Type,
    strategy: StaticRefStrategy,
) -> String {
    if type_is_location_capture(ty) {
        "copy `file` and `line` from Location; do not store &'static Location".to_string()
    } else if type_contains_static_str_ref(ty) {
        static_str_field_snippet(ty, strategy)
    } else {
        owned_static_ref_snippet(ty, strategy)
    }
}

#[instrument(level = "debug", skip(ty), ret)]
fn type_contains_static_str_ref(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => {
            let is_static = reference
                .lifetime
                .as_ref()
                .is_some_and(|lifetime| lifetime.ident == "static");
            (is_static && type_is_str(&reference.elem))
                || type_contains_static_str_ref(&reference.elem)
        }
        Type::Path(type_path) => type_path
            .path
            .segments
            .iter()
            .any(|segment| type_arguments_have_static_str_ref(&segment.arguments)),
        Type::Array(array) => type_contains_static_str_ref(&array.elem),
        Type::Slice(slice) => type_contains_static_str_ref(&slice.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(type_contains_static_str_ref),
        Type::Paren(paren) => type_contains_static_str_ref(&paren.elem),
        Type::Group(group) => type_contains_static_str_ref(&group.elem),
        Type::Ptr(pointer) => type_contains_static_str_ref(&pointer.elem),
        _ => false,
    }
}

#[instrument(level = "trace", skip(arguments), ret)]
fn type_arguments_have_static_str_ref(arguments: &PathArguments) -> bool {
    match arguments {
        PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| {
            matches!(arg, syn::GenericArgument::Type(inner) if type_contains_static_str_ref(inner))
        }),
        PathArguments::Parenthesized(args) => args
            .inputs
            .iter()
            .any(|inner| type_contains_static_str_ref(&inner.ty)),
        PathArguments::None => false,
    }
}

#[instrument(level = "debug", skip(ty), ret)]
fn type_is_str(ty: &Type) -> bool {
    match ty {
        Type::Paren(paren) => type_is_str(&paren.elem),
        Type::Group(group) => type_is_str(&group.elem),
        Type::Path(type_path) => type_path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "str"),
        _ => false,
    }
}

#[instrument(level = "debug", skip(ty, strategy))]
fn static_str_field_snippet(ty: &Type, strategy: StaticRefStrategy) -> String {
    let found = static_ref_snippet(ty);
    let message = match strategy {
        StaticRefStrategy::String => {
            format!("{found}; replace &'static str fields with owned String data")
        }
        StaticRefStrategy::Cow => {
            format!("{found}; replace &'static str fields with Cow<'static, str>")
        }
        StaticRefStrategy::Const => {
            format!(
                "{found}; const/static only for const-only types; otherwise use Cow<'static, str>"
            )
        }
    };
    truncate_snippet(&message, 128)
}

#[instrument(level = "debug", skip(ty, strategy))]
fn owned_static_ref_snippet(ty: &Type, strategy: StaticRefStrategy) -> String {
    let found = static_ref_snippet(ty);
    let message = match strategy {
        StaticRefStrategy::String => {
            format!("{found}; replace static reference fields with owned data")
        }
        StaticRefStrategy::Cow => {
            format!("{found}; use Cow or a domain wrapper only when borrowing is intentional")
        }
        StaticRefStrategy::Const => {
            format!("{found}; const/static only for const-only types; otherwise own the data")
        }
    };
    truncate_snippet(&message, 128)
}

#[instrument(level = "debug", skip(ty))]
fn static_ref_snippet(ty: &Type) -> String {
    truncate_snippet(&type_label_with_lifetime(ty), 96)
}

#[instrument(level = "debug", skip(ty))]
fn type_label_with_lifetime(ty: &Type) -> String {
    match ty {
        Type::Reference(reference) => {
            let mut out = String::from("&");
            if let Some(lifetime) = &reference.lifetime {
                out.push('\'');
                out.push_str(&lifetime.ident.to_string());
            }
            if reference.mutability.is_some() {
                out.push_str(" mut");
            }
            out.push(' ');
            out.push_str(&type_label_with_lifetime(&reference.elem));
            out
        }
        Type::Path(type_path) => {
            let mut segments = Vec::new();
            for segment in &type_path.path.segments {
                let mut label = segment.ident.to_string();
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    let inner: Vec<String> = args
                        .args
                        .iter()
                        .filter_map(|arg| match arg {
                            syn::GenericArgument::Type(inner) => {
                                Some(type_label_with_lifetime(inner))
                            }
                            syn::GenericArgument::Lifetime(lifetime) => {
                                Some(format!("'{}", lifetime.ident))
                            }
                            _ => None,
                        })
                        .collect();
                    if !inner.is_empty() {
                        label = format!("{label}<{inner}>", inner = inner.join(", "));
                    }
                }
                segments.push(label);
            }
            segments.join("::")
        }
        Type::Array(array) => format!("[{}; …]", type_label_with_lifetime(&array.elem)),
        Type::Slice(slice) => format!("[{}]", type_label_with_lifetime(&slice.elem)),
        Type::Tuple(tuple) => {
            let inner = tuple
                .elems
                .iter()
                .map(type_label_with_lifetime)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({inner})")
        }
        Type::Paren(paren) => type_label_with_lifetime(&paren.elem),
        Type::Group(group) => type_label_with_lifetime(&group.elem),
        _ => type_label(ty),
    }
}
