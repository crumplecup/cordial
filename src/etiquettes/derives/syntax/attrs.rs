//! Attribute and derive-name predicates.

use syn::{Attribute, Expr, ItemImpl};
use syn::{Token, punctuated::Punctuated};
use tracing::instrument;

use super::types::type_label;

#[instrument(level = "trace", skip(attrs), ret)]
pub(in crate::etiquettes::derives) fn has_track_caller(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident("track_caller"))
}

#[instrument(level = "debug", skip(item_impl))]
pub(in crate::etiquettes::derives) fn error_impl_target(item_impl: &ItemImpl) -> Option<String> {
    let (trait_path, _) = item_impl.trait_.as_ref()?;
    let last = trait_path.segments.last()?;
    if last.ident != "Error" {
        return None;
    }
    Some(type_label(&item_impl.self_ty))
}

#[instrument(level = "trace", skip(attrs))]
pub(in crate::etiquettes::derives) fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        list.tokens.to_string().replace(' ', "") == "test"
    })
}

/// Clap schema types: each field is a CLI argument, not an encapsulated record.
#[instrument(level = "trace", skip(attrs), ret)]
pub(in crate::etiquettes::derives) fn is_clap_schema(attrs: &[Attribute]) -> bool {
    has_derive(attrs, "Parser") || has_derive(attrs, "Args") || has_derive(attrs, "Subcommand")
}

/// `#[cfg(creusot)]`-only types: a real, if less common, reason a field
/// needs to stay at least as visible as it is. Creusot's own
/// proof-transparency check requires everything a spec clause touches to be
/// at least as visible as the function stating it.
///
/// This only recognizes the gate on the item's own attribute list.
/// `DeriveScanVisitor::walk_mod` covers ancestor `#[cfg(creusot)]` modules.
#[instrument(level = "trace", skip(attrs), ret)]
pub(in crate::etiquettes::derives) fn is_cfg_creusot(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        list.tokens.to_string().replace(' ', "") == "creusot"
    })
}

#[instrument(level = "trace", skip(attrs))]
pub(in crate::etiquettes::derives) fn has_derive(attrs: &[Attribute], needle: &str) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("derive") {
            return false;
        }
        let tokens = list.tokens.to_string();
        tokens.split(',').any(|part| {
            let compact = part.replace(' ', "");
            compact == needle
                || compact.ends_with(&format!("::{needle}"))
                || compact.contains(&format!("::{needle}::"))
        })
    })
}

#[instrument(level = "trace", skip(attrs))]
pub(in crate::etiquettes::derives) fn derive_builder_names(
    attrs: &[Attribute],
    struct_name: &str,
) -> Vec<String> {
    if !has_derive(attrs, "Builder") {
        return Vec::new();
    }
    let mut names = vec![format!("{struct_name}Builder")];
    for attr in attrs {
        let syn::Meta::List(list) = &attr.meta else {
            continue;
        };
        if !list.path.is_ident("builder") {
            continue;
        }
        if let Some(name) = builder_name_attr(attr) {
            names.push(name);
        }
    }
    names.sort();
    names.dedup();
    names
}

#[instrument(level = "trace", skip(attr))]
fn builder_name_attr(attr: &Attribute) -> Option<String> {
    let meta = attr
        .parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)
        .ok()?;
    meta.into_iter().find_map(|meta| {
        let syn::Meta::NameValue(name_value) = meta else {
            return None;
        };
        if !name_value.path.is_ident("name") {
            return None;
        }
        let Expr::Lit(lit) = name_value.value else {
            return None;
        };
        let syn::Lit::Str(name) = lit.lit else {
            return None;
        };
        Some(name.value())
    })
}
