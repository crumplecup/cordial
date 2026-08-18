//! Type and pattern predicates for antipattern rules.

use syn::spanned::Spanned;
use syn::{Pat, PathArguments, Type, TypeParamBound, TypePath, TypeTraitObject};

pub(super) struct UnusedArgBinding {
    pub(super) line: u32,
    pub(super) snippet: String,
}

pub(super) fn type_contains_static_lifetime_ref(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => {
            reference
                .lifetime
                .as_ref()
                .is_some_and(|lifetime| lifetime.ident == "static")
                || type_contains_static_lifetime_ref(&reference.elem)
        }
        Type::Path(type_path) => type_path
            .path
            .segments
            .iter()
            .any(|segment| match &segment.arguments {
                PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| {
                    matches!(arg, syn::GenericArgument::Type(inner) if type_contains_static_lifetime_ref(inner))
                }),
                PathArguments::Parenthesized(args) => args
                    .inputs
                    .iter()
                    .any(type_contains_static_lifetime_ref),
                PathArguments::None => false,
            }),
        Type::Array(array) => type_contains_static_lifetime_ref(&array.elem),
        Type::Slice(slice) => type_contains_static_lifetime_ref(&slice.elem),
        Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .any(type_contains_static_lifetime_ref),
        Type::Paren(paren) => type_contains_static_lifetime_ref(&paren.elem),
        Type::Group(group) => type_contains_static_lifetime_ref(&group.elem),
        Type::Ptr(pointer) => type_contains_static_lifetime_ref(&pointer.elem),
        Type::TraitObject(trait_obj) => trait_obj
            .bounds
            .iter()
            .any(|bound| match bound {
                TypeParamBound::Trait(trait_bound) => trait_bound
                    .path
                    .segments
                    .iter()
                    .any(|segment| match &segment.arguments {
                        PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| {
                            matches!(arg, syn::GenericArgument::Type(inner) if type_contains_static_lifetime_ref(inner))
                        }),
                        _ => false,
                    }),
                _ => false,
            }),
        _ => false,
    }
}

pub(super) fn static_ref_snippet(ty: &Type) -> String {
    truncate_snippet(&type_label_with_lifetime(ty), 96)
}

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

pub(super) fn unused_argument_bindings(pat: &Pat) -> Vec<UnusedArgBinding> {
    let mut bindings = Vec::new();
    collect_unused_argument_bindings(pat, &mut bindings);
    bindings
}

fn collect_unused_argument_bindings(pat: &Pat, bindings: &mut Vec<UnusedArgBinding>) {
    match pat {
        Pat::Wild(wild) => bindings.push(UnusedArgBinding {
            line: wild.span().start().line as u32,
            snippet: "_".to_string(),
        }),
        Pat::Ident(ident) if is_unused_argument_ident(&ident.ident) => {
            bindings.push(UnusedArgBinding {
                line: ident.span().start().line as u32,
                snippet: ident.ident.to_string(),
            });
        }
        Pat::Reference(reference) => collect_unused_argument_bindings(&reference.pat, bindings),
        Pat::Type(pat_type) => collect_unused_argument_bindings(&pat_type.pat, bindings),
        Pat::Paren(paren) => collect_unused_argument_bindings(&paren.pat, bindings),
        Pat::Tuple(tuple) => {
            for element in &tuple.elems {
                collect_unused_argument_bindings(element, bindings);
            }
        }
        Pat::TupleStruct(tuple_struct) => {
            for element in &tuple_struct.elems {
                collect_unused_argument_bindings(element, bindings);
            }
        }
        Pat::Struct(pat_struct) => {
            for field in &pat_struct.fields {
                collect_unused_argument_bindings(&field.pat, bindings);
            }
        }
        _ => {}
    }
}

fn is_unused_argument_ident(ident: &syn::Ident) -> bool {
    ident.to_string().starts_with('_')
}

pub(super) fn result_error_type(ty: &Type) -> Option<&Type> {
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

pub(super) fn is_stringish_error_type(ty: &Type) -> bool {
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

pub(super) fn result_string_error_snippet(ty: &Type) -> String {
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

pub(super) fn box_dyn_error_trait_object(ty: &Type) -> Option<&TypeTraitObject> {
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

pub(super) fn box_dyn_error_snippet(trait_obj: &TypeTraitObject) -> String {
    let bounds: Vec<String> = trait_obj.bounds.iter().map(trait_bound_label).collect();
    let snippet = format!("Box<dyn {}>", bounds.join(" + "));
    truncate_snippet(&snippet, 96)
}

fn trait_bound_label(bound: &TypeParamBound) -> String {
    match bound {
        TypeParamBound::Trait(trait_bound) => path_label(&trait_bound.path),
        TypeParamBound::Lifetime(lifetime) => lifetime.ident.to_string(),
        TypeParamBound::PreciseCapture(_) => "use<…>".to_string(),
        _ => "?".to_string(),
    }
}

pub(crate) fn truncate_snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

pub(super) fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

fn path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}
