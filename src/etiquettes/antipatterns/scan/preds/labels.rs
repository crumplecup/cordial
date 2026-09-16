//! Shared labels and snippet trimming for antipattern findings.

use syn::{Type, TypeParamBound};

use tracing::instrument;

#[instrument(level = "debug")]
pub(crate) fn truncate_snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

#[instrument(level = "debug", skip(ty))]
pub(in crate::etiquettes::antipatterns::scan) fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(bound))]
pub(in crate::etiquettes::antipatterns::scan) fn trait_bound_label(
    bound: &TypeParamBound,
) -> String {
    match bound {
        TypeParamBound::Trait(trait_bound) => path_label(&trait_bound.path),
        TypeParamBound::Lifetime(lifetime) => lifetime.ident.to_string(),
        TypeParamBound::PreciseCapture(_) => "use<…>".to_string(),
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
