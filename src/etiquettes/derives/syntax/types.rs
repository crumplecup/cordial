//! Type and path labels for derive evidence.

use syn::Type;
use tracing::instrument;

#[instrument(level = "debug", skip(ty))]
pub(in crate::etiquettes::derives) fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => syn_path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
fn syn_path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}
