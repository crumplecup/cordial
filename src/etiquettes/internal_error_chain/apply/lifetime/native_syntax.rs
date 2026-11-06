//! Exact native type, lifetime and source-location contracts for closed units.
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasAttrs, HasGenericArgs, HasGenericParams, HasTypeBounds},
};
use tracing::instrument;

#[instrument(level = "trace", skip(node))]
pub(super) fn range(node: &SyntaxNode) -> (usize, usize) {
    (
        node.text_range().start().into(),
        node.text_range().end().into(),
    )
}

#[instrument(level = "trace", skip(source))]
pub(super) fn position(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset];
    (
        prefix.bytes().filter(|byte| *byte == b'\n').count() + 1,
        prefix
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count(),
    )
}

#[instrument(level = "trace", skip(item))]
pub(super) fn generics_empty(item: &impl HasGenericParams) -> bool {
    item.generic_param_list()
        .is_none_or(|list| list.generic_params().next().is_none())
}

#[instrument(level = "trace", skip(item))]
pub(super) fn one_lifetime(item: &impl HasGenericParams) -> Option<String> {
    let params: Vec<_> = item.generic_param_list()?.generic_params().collect();
    let [ast::GenericParam::LifetimeParam(param)] = params.as_slice() else {
        return None;
    };
    (param.attrs().next().is_none() && param.type_bound_list().is_none())
        .then(|| {
            param.lifetime().map(|value| {
                value
                    .syntax()
                    .text()
                    .to_string()
                    .trim_start_matches('\'')
                    .to_string()
            })
        })
        .flatten()
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn single_segment(ty: &ast::Type) -> Option<ast::PathSegment> {
    let ast::Type::PathType(ty) = ty else {
        return None;
    };
    let path = ty.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    let segment = path.segment()?;
    segment.type_anchor().is_none().then_some(segment)
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn type_name(ty: ast::Type) -> Option<String> {
    single_segment(&ty)?
        .name_ref()
        .map(|name| name.text().to_string())
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn bare_type(ty: &ast::Type, name: &str) -> bool {
    let Some(segment) = single_segment(ty) else {
        return false;
    };
    matches!(ty, ast::Type::PathType(ty) if ty.path().is_some_and(|path| path.coloncolon_token().is_none()))
        && segment.coloncolon_token().is_none()
        && segment.name_ref().is_some_and(|value| value.text() == name)
        && segment.generic_arg_list().is_none()
        && segment.parenthesized_arg_list().is_none()
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn scalar(ty: ast::Type) -> Option<String> {
    let name = type_name(ty.clone())?;
    (single_segment(&ty).is_some_and(|part| {
        part.generic_arg_list().is_none() && part.parenthesized_arg_list().is_none()
    }) && matches!(
        name.as_str(),
        "bool"
            | "char"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
    ))
    .then_some(name)
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn str_ref(ty: &ast::Type, lifetime: &str) -> bool {
    matches!(ty, ast::Type::RefType(ty) if ty.mut_token().is_none()
        && ty.lifetime().is_some_and(|value| value.syntax().text().to_string() == format!("'{lifetime}"))
        && ty.ty().is_some_and(|ty| bare_type(&ty, "str")))
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn type_with_lifetime(ty: &ast::Type, name: &str, lifetime: &str) -> bool {
    let Some(segment) = single_segment(ty) else {
        return false;
    };
    let Some(list) = segment.generic_arg_list() else {
        return false;
    };
    let args: Vec<_> = list.generic_args().collect();
    segment.name_ref().is_some_and(|value| value.text() == name)
        && matches!(args.as_slice(), [ast::GenericArg::LifetimeArg(arg)]
            if arg.lifetime().is_some_and(|value| value.syntax().text().to_string() == format!("'{lifetime}")))
}
