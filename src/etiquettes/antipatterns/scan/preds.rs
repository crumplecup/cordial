//! Type and pattern predicates for antipattern rules.

use std::collections::HashSet;

use syn::spanned::Spanned;
use syn::{Pat, PathArguments, Type, TypeParamBound, TypePath, TypeTraitObject};

use tracing::instrument;
pub(super) struct UnusedArgBinding {
    pub(super) line: u32,
    pub(super) snippet: String,
}

/// True when a field type contains a `&'static` that is not a crate-local `dyn Trait`.
#[instrument(level = "debug", skip(ty, local_trait_names), ret)]
pub(super) fn type_contains_disallowed_static_ref(
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
        Type::Path(type_path) => {
            type_path
                .path
                .segments
                .iter()
                .any(|segment| match &segment.arguments {
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
                        .any(|inner| type_contains_disallowed_static_ref(inner, local_trait_names)),
                    PathArguments::None => false,
                })
        }
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

#[instrument(level = "trace", skip(trait_obj, local_trait_names), ret)]
fn trait_object_has_disallowed_static_ref(
    trait_obj: &TypeTraitObject,
    local_trait_names: &HashSet<String>,
) -> bool {
    trait_obj.bounds.iter().any(|bound| match bound {
        TypeParamBound::Trait(trait_bound) => {
            trait_bound
                .path
                .segments
                .iter()
                .any(|segment| match &segment.arguments {
                    PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| {
                        matches!(
                            arg,
                            syn::GenericArgument::Type(inner)
                                if type_contains_disallowed_static_ref(inner, local_trait_names)
                        )
                    }),
                    _ => false,
                })
        }
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
pub(super) fn type_is_location_capture(ty: &Type) -> bool {
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

#[instrument(level = "debug", skip(ty))]
pub(super) fn static_ref_field_snippet(ty: &Type) -> String {
    if type_is_location_capture(ty) {
        "copy `file` and `line` from Location; do not store &'static Location".to_string()
    } else {
        static_ref_snippet(ty)
    }
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

#[instrument(level = "debug", skip(pat))]
pub(super) fn unused_argument_bindings(pat: &Pat) -> Vec<UnusedArgBinding> {
    let mut bindings = Vec::new();
    collect_unused_argument_bindings(pat, &mut bindings);
    bindings
}

#[instrument(level = "debug", skip(pat, bindings))]
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

#[instrument(level = "trace", skip(ident), ret)]
fn is_unused_argument_ident(ident: &syn::Ident) -> bool {
    ident.to_string().starts_with('_')
}

/// Whether `attrs` marks a function whose parameter list is a compiler-
/// mandated ABI, not a shape its author chose: `#[proc_macro_attribute]`
/// (always exactly `(TokenStream, TokenStream)`), `#[proc_macro_derive]`,
/// or `#[proc_macro]`. Same reasoning as skipping a foreign trait impl's
/// signature -- the parameter list isn't this function's own to shrink --
/// just enforced by the macro system instead of a trait declaration.
#[instrument(level = "debug", skip(attrs), ret)]
pub(super) fn has_proc_macro_abi_attr(attrs: &[syn::Attribute]) -> bool {
    ["proc_macro_attribute", "proc_macro_derive", "proc_macro"]
        .iter()
        .any(|name| attrs.iter().any(|attr| attr.path().is_ident(name)))
}

/// Whether `attrs` + `block` together mark a Creusot `#[trusted]
/// #[logic(opaque)]` axiom stub: an uninterpreted logic function whose
/// parameters exist only to make the axiom parametric across call sites
/// (Pearlite substitutes the caller's own expression for each one), never
/// to be read by the body -- `dead` is Creusot's own sentinel body for
/// exactly this idiom, confirmed against real sites in both this
/// workspace and `elicitation_creusot::logic_fns.rs`. Checking both the
/// attribute and the body shape (not either alone) keeps this narrow:
/// `#[logic(opaque)]` alone doesn't guarantee a `dead` body, and a
/// function that merely happens to reference an identifier named `dead`
/// without the attribute isn't this pattern.
#[instrument(level = "debug", skip(attrs, block), ret)]
pub(super) fn is_creusot_opaque_logic_stub(attrs: &[syn::Attribute], block: &syn::Block) -> bool {
    let has_logic_opaque = attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        list.path.is_ident("logic") && list.tokens.to_string().replace(' ', "") == "opaque"
    });
    if !has_logic_opaque {
        return false;
    }
    matches!(
        block.stmts.as_slice(),
        [syn::Stmt::Expr(syn::Expr::Path(expr_path), None)] if expr_path.path.is_ident("dead")
    )
}

#[instrument(level = "debug", skip(ty))]
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

#[instrument(level = "trace", skip(ty))]
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

#[instrument(level = "debug", skip(ty))]
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

#[instrument(level = "debug", skip(ty))]
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
pub(super) fn box_dyn_error_snippet(trait_obj: &TypeTraitObject) -> String {
    let bounds: Vec<String> = trait_obj.bounds.iter().map(trait_bound_label).collect();
    let snippet = format!("Box<dyn {}>", bounds.join(" + "));
    truncate_snippet(&snippet, 96)
}

#[instrument(level = "debug", skip(bound))]
fn trait_bound_label(bound: &TypeParamBound) -> String {
    match bound {
        TypeParamBound::Trait(trait_bound) => path_label(&trait_bound.path),
        TypeParamBound::Lifetime(lifetime) => lifetime.ident.to_string(),
        TypeParamBound::PreciseCapture(_) => "use<…>".to_string(),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug")]
pub(crate) fn truncate_snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

#[instrument(level = "debug", skip(ty))]
pub(super) fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
fn path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}
