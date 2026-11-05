use ra_ap_syntax::{AstNode, ast, ast::HasAttrs};

#[tracing::instrument(level = "trace", skip(owner))]
pub(super) fn cfg_flag(owner: &dyn HasAttrs, flag: &str) -> bool {
    owner.attrs().any(|attr| {
        attr.syntax()
            .text()
            .to_string()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>()
            == format!("#[cfg({flag})]")
    })
}

#[tracing::instrument(level = "trace", skip(path))]
pub(super) fn path_name(path: &ast::Path) -> Option<String> {
    path.segment()?
        .name_ref()
        .map(|name| name.text().to_string())
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn type_label(ty: &ast::Type) -> String {
    match ty {
        ast::Type::PathType(ty) => ty
            .path()
            .as_ref()
            .and_then(path_name)
            .unwrap_or_else(|| "?".into()),
        ast::Type::RefType(ty) => ty
            .ty()
            .as_ref()
            .map(type_label)
            .unwrap_or_else(|| "?".into()),
        ast::Type::ParenType(ty) => ty
            .ty()
            .as_ref()
            .map(type_label)
            .unwrap_or_else(|| "?".into()),
        _ => "?".into(),
    }
}

#[tracing::instrument(level = "trace", skip(function))]
pub(super) fn tracked_function(function: &ast::Fn) -> bool {
    !function
        .syntax()
        .parent()
        .and_then(|parent| parent.parent())
        .is_some_and(|owner| ast::Trait::can_cast(owner.kind()))
}
