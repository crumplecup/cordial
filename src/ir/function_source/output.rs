//! Recognize the exact wrapper emitted by the return-lifting operation.

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
