//! Context labels preserve the existing policy contracts.

use ra_ap_syntax::ast;

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn type_label(ty: ast::Type) -> String {
    match ty {
        ast::Type::PathType(ty) => ty
            .path()
            .and_then(|path| path.segment())
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string()),
        ast::Type::RefType(ty) => return ty.ty().map(type_label).unwrap_or_else(|| "?".into()),
        ast::Type::ParenType(ty) => return ty.ty().map(type_label).unwrap_or_else(|| "?".into()),
        _ => None,
    }
    .unwrap_or_else(|| "?".into())
}

#[cfg(any(
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "modularity"
))]
mod owners;
#[cfg(feature = "cfg_hygiene")]
pub(super) use owners::hygiene;
#[cfg(feature = "cfg_scatter")]
pub(super) use owners::label;
#[cfg(any(feature = "allows", feature = "cfg_scatter"))]
pub(super) use owners::site;
#[cfg(feature = "modularity")]
pub(super) use owners::{function_context, joined};
