//! Recognize the exact wrapper emitted by the return-lifting operation.

use serde::{Deserialize, Serialize};
use syn::spanned::Spanned;

/// Syntactic entry signature; Cargo identity must be checked separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceEntryContract {
    /// A free `main` with no parameters and a unit return.
    UnitMain,
}

#[tracing::instrument(level = "trace", skip(function))]
pub(super) fn entry_contract(function: &syn::ItemFn) -> Option<SourceEntryContract> {
    let unit = match &function.sig.output {
        syn::ReturnType::Default => true,
        syn::ReturnType::Type(_, ty) => {
            matches!(ty.as_ref(), syn::Type::Tuple(tuple) if tuple.elems.is_empty())
        }
    };
    (function.sig.ident == "main" && function.sig.inputs.is_empty() && unit)
        .then_some(SourceEntryContract::UnitMain)
}

/// Explicit absolute standard Result syntax and its source type arguments.
///
/// This deliberately leaves aliases, relative paths, and generic error types
/// unresolved. `error_identity` is available only for an absolute nongeneric
/// standard type. Namespace and callee checks must precede any rewrite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceResultContract {
    success_syntax: String,
    error_syntax: String,
    error_identity: Option<String>,
}

#[tracing::instrument(level = "trace", skip(file, output))]
pub(super) fn result_contract(
    file: &crate::loader::SharedParsedFile,
    output: &syn::ReturnType,
) -> Option<SourceResultContract> {
    let syn::ReturnType::Type(_, ty) = output else {
        return None;
    };
    let syn::Type::Path(path) = ty.as_ref() else {
        return None;
    };
    if path.qself.is_some()
        || !(absolute_path_is(&path.path, &["std", "result", "Result"])
            || absolute_path_is(&path.path, &["core", "result", "Result"]))
    {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &path.path.segments.last()?.arguments
    else {
        return None;
    };
    let mut args = arguments.args.iter();
    let syn::GenericArgument::Type(success) = args.next()? else {
        return None;
    };
    let syn::GenericArgument::Type(error) = args.next()? else {
        return None;
    };
    if args.next().is_some() {
        return None;
    }
    Some(SourceResultContract {
        success_syntax: file.source()[success.span().byte_range()].to_string(),
        error_syntax: file.source()[error.span().byte_range()].to_string(),
        error_identity: standard_type_identity(error),
    })
}

#[tracing::instrument(level = "trace", skip(ty))]
fn standard_type_identity(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    if path.qself.is_some()
        || path.path.leading_colon.is_none()
        || !path
            .path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "std" || segment.ident == "core")
        || path
            .path
            .segments
            .iter()
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
    {
        return None;
    }
    Some(format!(
        "::{}",
        path.path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::")
    ))
}

/// This is syntax recognition for repeat planning, not type resolution.
#[tracing::instrument(level = "trace", skip(output))]
pub(super) fn has_infallible_result_syntax(output: &syn::ReturnType) -> bool {
    let syn::ReturnType::Type(_, ty) = output else {
        return false;
    };
    let syn::Type::Path(ty) = ty.as_ref() else {
        return false;
    };
    if ty.qself.is_some() || !absolute_path_is(&ty.path, &["core", "result", "Result"]) {
        return false;
    }
    let Some(segment) = ty.path.segments.last() else {
        return false;
    };
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return false;
    };
    if arguments.args.len() != 2 {
        return false;
    }
    let Some(syn::GenericArgument::Type(syn::Type::Path(error))) = arguments.args.last() else {
        return false;
    };
    error.qself.is_none()
        && absolute_path_is(&error.path, &["core", "convert", "Infallible"])
        && error
            .path
            .segments
            .iter()
            .all(|segment| matches!(segment.arguments, syn::PathArguments::None))
}

#[tracing::instrument(level = "trace", skip(path, names))]
fn absolute_path_is(path: &syn::Path, names: &[&str]) -> bool {
    path.leading_colon.is_some()
        && path.segments.len() == names.len()
        && path
            .segments
            .iter()
            .zip(names)
            .all(|(segment, name)| segment.ident == *name)
        && path
            .segments
            .iter()
            .take(names.len().saturating_sub(1))
            .all(|segment| matches!(segment.arguments, syn::PathArguments::None))
}
