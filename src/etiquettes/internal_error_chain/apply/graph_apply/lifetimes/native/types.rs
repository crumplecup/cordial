//! Exact generic, field and type-use contracts for lifetime planning.
use super::super::{ConditionalScope, Range, TypeArguments, TypePathSite};
use super::Scope;
use crate::etiquettes::internal_error_chain::apply::graph_apply::syntax;
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasAttrs, HasGenericArgs, HasGenericParams, HasTypeBounds},
};
use std::collections::BTreeSet;
use tracing::instrument;

#[instrument(level = "trace", skip(node))]
pub(super) fn range(node: &SyntaxNode) -> Range {
    let range = node.text_range();
    (range.start().into(), range.end().into())
}

#[instrument(level = "trace", skip(value))]
pub(super) fn lifetime_name(value: &ast::Lifetime) -> String {
    value
        .syntax()
        .text()
        .to_string()
        .trim_start_matches('\'')
        .to_string()
}

#[instrument(level = "trace", skip(node))]
pub(super) fn lifetimes(node: &SyntaxNode) -> BTreeSet<String> {
    node.descendants()
        .filter(|node| {
            !node
                .ancestors()
                .any(|node| matches!(node.kind(), SyntaxKind::TOKEN_TREE | SyntaxKind::ATTR))
        })
        .filter_map(ast::Lifetime::cast)
        .map(|value| lifetime_name(&value))
        .collect()
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
        .then(|| param.lifetime().map(|value| lifetime_name(&value)))
        .flatten()
}

#[instrument(level = "trace", skip(item))]
pub(super) fn generics_range(item: &impl HasGenericParams) -> Option<Range> {
    Some(
        item.generic_param_list()
            .map_or((0, 0), |list| range(list.syntax())),
    )
}

#[instrument(level = "trace", skip(path))]
pub(super) fn trait_path(path: &ast::PathType) -> bool {
    path.syntax().parent().is_some_and(|parent| {
        if ast::TypeBound::can_cast(parent.kind()) {
            return true;
        }
        if let Some(item) = ast::Impl::cast(parent.clone()) {
            return item.trait_().is_some_and(|ty| ty.syntax() == path.syntax());
        }
        ast::TypeAnchor::cast(parent).is_some_and(|anchor| {
            anchor.as_token().is_some()
                && anchor
                    .syntax()
                    .children()
                    .filter_map(ast::Type::cast)
                    .last()
                    .is_some_and(|ty| ty.syntax() == path.syntax())
        })
    })
}

#[instrument(level = "trace", skip(path, scope))]
pub(super) fn path_site(path: &ast::PathType, scope: &Scope) -> Option<TypePathSite> {
    let path_node = path.path()?;
    let segment = path_node.segment()?;
    let name = segment.name_ref()?;
    let arguments = if let Some(args) = segment.generic_arg_list() {
        let all: Vec<_> = args.generic_args().collect();
        match all.as_slice() {
            [ast::GenericArg::LifetimeArg(arg)] => TypeArguments::OneLifetime {
                name: lifetime_name(&arg.lifetime()?),
                range: Some(range(args.syntax())),
            },
            _ => TypeArguments::Other,
        }
    } else if segment.parenthesized_arg_list().is_some() {
        TypeArguments::Other
    } else {
        TypeArguments::None
    };
    let mut qself = false;
    let mut next = Some(path_node.clone());
    while let Some(path) = next {
        qself |= path
            .segment()
            .is_some_and(|part| part.type_anchor().is_some());
        next = path.qualifier();
    }
    Some(TypePathSite {
        name: name.text().to_string(),
        range: Some(range(path.syntax())),
        ident_end: Some(name.syntax().text_range().end().into()),
        arguments,
        qself,
        path_len: syntax::names(path_node).len(),
        scope_lifetimes: scope.lifetimes.clone(),
        conditional_scope: scope.conditional.as_ref().map(|value| ConditionalScope {
            owner: value.owner.clone(),
            borrowed_only: value.borrowed_only,
        }),
    })
}

#[instrument(level = "trace", skip(ty))]
fn has_storage(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::RefType(reference) => {
            reference.mut_token().is_none() && reference.ty().and_then(syntax::type_name).is_some()
        }
        ast::Type::PathType(path) => {
            let Some(path) = path.path() else {
                return false;
            };
            if syntax::names(path.clone()) == ["Box"] {
                let args: Vec<_> = path
                    .segment()
                    .and_then(|part| part.generic_arg_list())
                    .into_iter()
                    .flat_map(|args| args.generic_args())
                    .collect();
                matches!(args.as_slice(), [ast::GenericArg::TypeArg(arg)] if arg.ty().and_then(syntax::type_name).is_some())
            } else {
                syntax::type_name(ty.clone()).is_some()
            }
        }
        _ => false,
    }
}

#[instrument(level = "trace", skip(name, ty))]
pub(super) fn field_needs(name: Option<&str>, ty: &ast::Type, owned: bool) -> bool {
    if name == Some("file") || !has_storage(ty) {
        return false;
    }
    if owned {
        matches!(ty, ast::Type::RefType(_))
            && (name.is_none()
                || matches!(name, Some("kind" | "source"))
                || syntax::type_name(ty.clone()).as_deref() == Some("str"))
    } else {
        if matches!(ty, ast::Type::RefType(_)) {
            return false;
        }
        !matches!(ty, ast::Type::PathType(ty) if ty.path().is_some_and(|path| path.qualifier().is_none() && path.coloncolon_token().is_none() && path.segment().is_some_and(|segment| segment.generic_arg_list().is_none() && segment.name_ref().is_some_and(|name| matches!(name.text(), "bool" | "char" | "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize")))))
    }
}
