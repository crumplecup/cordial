//! Name-use helpers for the conversion-site visitor.

use syn::{PathArguments, Type};
use tracing::instrument;

/// Only these familiar derives have no hidden constructor or alias path.
#[instrument(level = "debug", skip(attr))]
pub(super) fn derive_is_nonconstructing(attr: &syn::Attribute) -> bool {
    derive_matches(attr, true)
}

/// Restrict declaration links to compiler-provided derives.
#[instrument(level = "debug", skip(attr))]
pub(in crate::etiquettes::internal_error_chain::apply) fn derive_is_builtin(
    attr: &syn::Attribute,
) -> bool {
    derive_matches(attr, false)
}

#[instrument(level = "trace", skip(attr))]
fn derive_matches(attr: &syn::Attribute, allow_error: bool) -> bool {
    if !attr.path().is_ident("derive") {
        return false;
    }
    let mut known = true;
    attr.parse_nested_meta(|meta| {
        known &= meta.path.segments.len() == 1
            && meta.path.segments.last().is_some_and(|part| {
                matches!(
                    part.ident.to_string().as_str(),
                    "Debug" | "Clone" | "Copy" | "PartialEq" | "Eq" | "Hash" | "PartialOrd" | "Ord"
                ) || (allow_error && part.ident == "Error")
            });
        Ok(())
    })
    .is_ok()
        && known
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn plain_type_path(ty: &Type) -> Option<Vec<String>> {
    let Type::Path(path) = ty else { return None };
    if path.qself.is_some()
        || path.path.segments.is_empty()
        || path.path.segments.iter().any(|part| {
            !matches!(&part.arguments, PathArguments::None)
                && !matches!(&part.arguments, PathArguments::AngleBracketed(arguments)
                    if arguments.args.iter().all(|argument| matches!(argument, syn::GenericArgument::Lifetime(_))))
        })
    {
        return None;
    }
    Some(
        path.path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect(),
    )
}
