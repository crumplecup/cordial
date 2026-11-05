//! Native field type and declaration policy shapes.
use super::super::Visibility;

use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasAttrs, HasGenericArgs, HasGenericParams, HasTypeBounds},
};

#[tracing::instrument(level = "trace", skip(file, node))]
pub(super) fn start_line(file: &DatabaseSource<'_>, node: &SyntaxNode) -> usize {
    let start = node
        .descendants_with_tokens()
        .filter_map(|part| part.into_token())
        .find(|token| !token.kind().is_trivia())
        .map_or(node.text_range().start(), |token| {
            token.text_range().start()
        });
    file.line(start) as usize
}

#[tracing::instrument(level = "trace", skip(owner))]
pub(super) fn allowed_attrs(owner: &impl HasAttrs) -> bool {
    owner.attrs().all(|attr| {
        if matches!(attr.meta(), Some(ast::Meta::UnsafeMeta(_)))
            || attr
                .path()
                .is_some_and(|path| path.coloncolon_token().is_some())
        {
            return false;
        }
        match attr.simple_name().as_deref() {
            Some("doc") => return true,
            Some("derive") => {}
            _ => return false,
        }
        let Some(ast::Meta::TokenTreeMeta(meta)) = attr.meta() else {
            return false;
        };
        let Some(tree) = meta.token_tree() else {
            return false;
        };
        let tokens: Vec<_> = tree
            .syntax()
            .children_with_tokens()
            .filter(|token| !token.kind().is_trivia())
            .collect();
        let [_, contents @ .., _] = tokens.as_slice() else {
            return false;
        };
        let mut groups = contents.split(|token| token.to_string() == ",").peekable();
        while let Some(group) = groups.next() {
            if group.is_empty() && groups.peek().is_none() {
                continue;
            }
            let [name] = group else {
                return false;
            };
            if !matches!(
                name.to_string().as_str(),
                "Debug" | "Clone" | "Copy" | "PartialEq" | "Eq"
            ) {
                return false;
            }
        }
        true
    })
}

/// Identity links permit compiler derives only; doc attributes still block them.
#[tracing::instrument(level = "trace", skip(owner))]
pub(super) fn builtin_attrs(owner: &impl HasAttrs) -> bool {
    owner.doc_comments().next().is_none()
        && owner.attrs().all(|attr| {
            if attr.simple_name().as_deref() != Some("derive")
                || attr
                    .path()
                    .is_some_and(|path| path.coloncolon_token().is_some())
            {
                return false;
            }
            let Some(ast::Meta::TokenTreeMeta(meta)) = attr.meta() else {
                return false;
            };
            let Some(tree) = meta.token_tree() else {
                return false;
            };
            let tokens: Vec<_> = tree
                .syntax()
                .children_with_tokens()
                .filter(|token| !token.kind().is_trivia())
                .collect();
            let [_, contents @ .., _] = tokens.as_slice() else {
                return false;
            };
            let mut groups = contents.split(|token| token.to_string() == ",").peekable();
            while let Some(group) = groups.next() {
                if group.is_empty() && groups.peek().is_none() {
                    continue;
                }
                let text: String = group.iter().map(ToString::to_string).collect();
                if !matches!(
                    text.strip_prefix("::").unwrap_or(&text),
                    "Debug" | "Clone" | "Copy" | "PartialEq" | "Eq" | "Hash" | "PartialOrd" | "Ord"
                ) {
                    return false;
                }
            }
            true
        })
}

#[tracing::instrument(level = "trace", skip(value))]
pub(super) fn visibility(value: Option<ast::Visibility>) -> Visibility {
    match value {
        None => Visibility::Inherited,
        Some(vis) if vis.visibility_inner().is_some() => Visibility::Restricted,
        Some(_) => Visibility::Public,
    }
}

#[tracing::instrument(level = "trace", skip(file, node))]
pub(super) fn column(file: &DatabaseSource<'_>, node: &SyntaxNode) -> usize {
    let offset = usize::from(node.text_range().start());
    let start = file.source()[..offset]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    file.source()[start..offset].chars().count()
}

#[tracing::instrument(level = "trace", skip(params))]
pub(super) fn only_lifetime(params: Option<&ast::GenericParamList>) -> Option<String> {
    let mut lifetimes = params?.generic_params().filter_map(|param| {
        if let ast::GenericParam::LifetimeParam(param) = param {
            param.lifetime()
        } else {
            None
        }
    });
    let first = lifetime_name(&lifetimes.next()?);
    lifetimes.next().is_none().then_some(first)
}

#[tracing::instrument(level = "trace", skip(item))]
pub(super) fn single_lifetime(item: &ast::Struct) -> Option<String> {
    if item.where_clause().is_some() {
        return None;
    }
    let params: Vec<_> = item.generic_param_list()?.generic_params().collect();
    let [ast::GenericParam::LifetimeParam(param)] = params.as_slice() else {
        return None;
    };
    if param.attrs().next().is_some() || param.type_bound_list().is_some() {
        return None;
    }
    let name = lifetime_name(&param.lifetime()?);
    let fields: Vec<_> = match item.field_list()? {
        ast::FieldList::RecordFieldList(fields) => {
            fields.fields().filter_map(|field| field.ty()).collect()
        }
        ast::FieldList::TupleFieldList(fields) => {
            fields.fields().filter_map(|field| field.ty()).collect()
        }
    };
    fields
        .into_iter()
        .any(|ty| {
            matches!(ty, ast::Type::RefType(reference)
        if reference.lifetime().is_some_and(|lifetime| lifetime_name(&lifetime) == name)
            && reference.ty().is_some_and(|ty| !is_plain(&ty, "str")))
        })
        .then_some(name)
}

#[tracing::instrument(level = "trace", skip(value))]
pub(super) fn lifetime_name(value: &ast::Lifetime) -> String {
    value
        .syntax()
        .text()
        .to_string()
        .trim_start_matches('\'')
        .to_string()
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn path_parts(ty: &ast::Type) -> Option<Vec<ast::PathSegment>> {
    let ast::Type::PathType(ty) = ty else {
        return None;
    };
    let mut current = ty.path();
    let mut parts = Vec::new();
    while let Some(path) = current {
        let segment = path.segment()?;
        segment.name_ref()?;
        if segment.type_anchor().is_some() {
            return None;
        }
        parts.push(segment);
        current = path.qualifier();
    }
    parts.reverse();
    Some(parts)
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn is_plain(ty: &ast::Type, name: &str) -> bool {
    let ast::Type::PathType(path_type) = ty else {
        return false;
    };
    path_parts(ty).is_some_and(|parts| {
        parts.len() == 1
            && parts[0].name_ref().is_some_and(|part| part.text() == name)
            && parts[0].generic_arg_list().is_none()
            && parts[0].parenthesized_arg_list().is_none()
            && parts[0].coloncolon_token().is_none()
    }) && path_type
        .path()
        .is_some_and(|path| path.coloncolon_token().is_none())
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn type_name(ty: &ast::Type) -> Option<String> {
    if let ast::Type::RefType(reference) = ty {
        return type_name(&reference.ty()?);
    }
    let parts = path_parts(ty)?;
    if parts.len() == 1 && parts[0].name_ref()?.text() == "Box" {
        let ast::GenericArg::TypeArg(inner) = parts[0].generic_arg_list()?.generic_args().next()?
        else {
            return None;
        };
        return type_name(&inner.ty()?);
    }
    Some(parts.last()?.name_ref()?.text().to_string())
}
