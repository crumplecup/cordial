use syn::{Expr, ExprLit, ExprMethodCall, Lit, Macro, Type};

use super::PanicKind;

use tracing::instrument;

#[instrument(level = "trace", skip(call), ret)]
pub(super) fn is_unwrap_variant(call: &ExprMethodCall) -> bool {
    matches!(
        call.method.to_string().as_str(),
        "unwrap_or" | "unwrap_or_else" | "unwrap_or_default"
    )
}

#[instrument(level = "trace", skip(attrs))]
pub(super) fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    has_cfg_flag(attrs, "test")
}

/// Whether `attrs` carries a bare `#[cfg(flag)]` (no `not(..)`, no
/// `all(..)`/`any(..)` combinators -- just the single flag name).
#[instrument(level = "trace", skip(attrs, flag))]
pub(in crate::etiquettes::panics) fn has_cfg_flag(attrs: &[syn::Attribute], flag: &str) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        list.tokens.to_string().replace(' ', "") == flag
    })
}

/// Whether `attrs` carries a bare `#[cfg(not(flag))]`.
#[instrument(level = "trace", skip(attrs, flag))]
pub(super) fn has_cfg_not_flag(attrs: &[syn::Attribute], flag: &str) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        list.tokens.to_string().replace(' ', "") == format!("not({flag})")
    })
}

/// Cheap prefilter so every string literal is not run through `syn`.
#[instrument(level = "debug", skip(source))]
pub(super) fn looks_like_embedded_panic_source(source: &str) -> bool {
    source.contains("panic!")
        || source.contains("unreachable!")
        || source.contains("compile_error!")
        || source.contains(".unwrap(")
        || source.contains(".unwrap_err(")
        || source.contains(".expect(")
        || source.contains(".expect_err(")
}

#[instrument(level = "debug", skip(path))]
pub(super) fn macro_panic_kind(path: &syn::Path) -> Option<PanicKind> {
    let ident = path.segments.last()?.ident.to_string();
    match ident.as_str() {
        "panic" => Some(PanicKind::Panic),
        "unreachable" => Some(PanicKind::Unreachable),
        "compile_error" => Some(PanicKind::CompileError),
        _ => None,
    }
}

#[instrument(level = "debug", skip(mac))]
pub(super) fn macro_snippet(mac: &Macro) -> String {
    let name = path_label(&mac.path);
    let args = mac.tokens.to_string();
    let trimmed = truncate_snippet(&args, 72);
    format!("{name}!({trimmed})")
}

#[instrument(level = "debug", skip(call))]
pub(super) fn expect_snippet(call: &ExprMethodCall) -> String {
    if let Some(Expr::Lit(ExprLit {
        lit: Lit::Str(lit), ..
    })) = call.args.first()
    {
        return format!(".{}(\"{}\")", call.method, lit.value());
    }
    format!(".{}(…)", call.method)
}

#[instrument(level = "debug")]
fn truncate_snippet(text: &str, max: usize) -> String {
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
