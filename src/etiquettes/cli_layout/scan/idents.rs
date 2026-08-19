//! Syn type-ident helpers for clap catalog and act hunting.

use std::collections::BTreeMap;

use syn::{Attribute, Fields, FnArg, ReturnType, Signature, Type};

use super::super::tree::{last_ident, type_label};
use super::catalog::VariantShape;

use tracing::instrument;
#[instrument(level = "debug", skip(fields))]
pub(super) fn named_field_map(fields: &Fields) -> BTreeMap<String, Vec<String>> {
    match fields {
        Fields::Named(named) => named
            .named
            .iter()
            .filter_map(|field| {
                Some((
                    field.ident.as_ref()?.to_string(),
                    collect_type_idents(&field.ty),
                ))
            })
            .collect(),
        Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(index, field)| (index.to_string(), collect_type_idents(&field.ty)))
            .collect(),
        Fields::Unit => BTreeMap::new(),
    }
}

#[instrument(level = "debug", skip(fields))]
pub(super) fn variant_shape(fields: &Fields) -> VariantShape {
    match fields {
        Fields::Named(_) => VariantShape::Named(named_field_map(fields)),
        Fields::Unnamed(unnamed) => VariantShape::Unnamed(
            unnamed
                .unnamed
                .iter()
                .map(|field| collect_type_idents(&field.ty))
                .collect(),
        ),
        Fields::Unit => VariantShape::Unit,
    }
}

#[instrument(level = "debug", skip(ty))]
pub(super) fn collect_type_idents(ty: &Type) -> Vec<String> {
    let mut out = Vec::new();
    push_type_idents(ty, &mut out);
    out
}

#[instrument(level = "debug", skip(ty))]
fn push_type_idents(ty: &Type, out: &mut Vec<String>) {
    match ty {
        Type::Path(path) => {
            if let Some(last) = path.path.segments.last() {
                out.push(last.ident.to_string());
            }
            for segment in &path.path.segments {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    for arg in &args.args {
                        if let syn::GenericArgument::Type(inner) = arg {
                            push_type_idents(inner, out);
                        }
                    }
                }
            }
        }
        Type::Reference(reference) => push_type_idents(&reference.elem, out),
        Type::Paren(paren) => push_type_idents(&paren.elem, out),
        Type::Group(group) => push_type_idents(&group.elem, out),
        Type::Tuple(tuple) => {
            for elem in &tuple.elems {
                push_type_idents(elem, out);
            }
        }
        Type::Slice(slice) => push_type_idents(&slice.elem, out),
        Type::Array(array) => push_type_idents(&array.elem, out),
        Type::Ptr(ptr) => push_type_idents(&ptr.elem, out),
        _ => {}
    }
}

#[instrument(level = "debug", skip(sig))]
pub(super) fn input_type_idents(sig: &Signature) -> Vec<String> {
    sig.inputs
        .iter()
        .filter_map(|arg| {
            let FnArg::Typed(pat) = arg else {
                return None;
            };
            Some(collect_type_idents(&pat.ty))
        })
        .flatten()
        .collect()
}

#[instrument(level = "trace", skip(sig))]
pub(super) fn has_self_receiver(sig: &Signature) -> bool {
    sig.inputs.iter().any(|arg| match arg {
        FnArg::Receiver(_) => true,
        FnArg::Typed(pat) => {
            matches!(pat.ty.as_ref(), Type::Path(path) if path.path.is_ident("Self"))
        }
    })
}

#[instrument(level = "debug", skip(attrs))]
pub(super) fn item_derives(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == name)
            {
                found = true;
            }
            Ok(())
        });
        found
    })
}

#[instrument(level = "debug", skip(sig))]
pub(super) fn sig_returns_result(sig: &Signature) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => {
            let label = type_label(ty);
            let last = last_ident(&label);
            last == "Result" || last.ends_with("Result")
        }
        ReturnType::Default => false,
    }
}
