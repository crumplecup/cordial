//! Storage-independent getter, Error and conditional Display annotations.
use super::imports;
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasAttrs},
};

#[tracing::instrument(level = "trace", skip(attr))]
pub(in crate::etiquettes::internal_error_chain::apply) fn named(
    attr: &ast::Attr,
    name: &str,
) -> bool {
    !matches!(attr.meta(), Some(ast::Meta::UnsafeMeta(_)))
        && attr.simple_name().as_deref() == Some(name)
        && attr.path().is_none_or(|path| {
            path.qualifier().is_none()
                && path.coloncolon_token().is_none()
                && path
                    .segment()
                    .is_some_and(|segment| segment.coloncolon_token().is_none())
        })
}

#[tracing::instrument(level = "trace", skip(attr))]
pub(super) fn argument(attr: &ast::Attr, name: &str, value: &str) -> bool {
    if !named(attr, name) {
        return false;
    }
    let Some(ast::Meta::TokenTreeMeta(meta)) = attr.meta() else {
        return false;
    };
    let Some(tree) = meta.token_tree() else {
        return false;
    };
    if tree.l_paren_token().is_none() {
        return false;
    }
    let tokens: Vec<_> = tree
        .syntax()
        .children_with_tokens()
        .filter(|t| !t.kind().is_trivia())
        .collect();
    matches!(tokens.as_slice(), [_, token, _] if token.kind() == SyntaxKind::IDENT && token.to_string() == value)
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn bare_type(ty: &ast::Type) -> Option<String> {
    let ast::Type::PathType(ty) = ty else {
        return None;
    };
    let path = ty.path()?;
    if path.qualifier().is_some()
        || path.coloncolon_token().is_some()
        || path.segment()?.coloncolon_token().is_some()
    {
        return None;
    }
    let names = imports::segments(&path)?;
    match names.as_slice() {
        [name] => Some(name.clone()),
        _ => None,
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn static_str(ty: &ast::Type) -> bool {
    let ast::Type::RefType(ty) = ty else {
        return false;
    };
    ty.mut_token().is_none()
        && ty.lifetime().is_some_and(|name| name.text() == "'static")
        && ty.ty().and_then(|ty| bare_type(&ty)).as_deref() == Some("str")
}

#[tracing::instrument(level = "trace", skip(file, owner, field))]
pub(super) fn field_stable(
    file: &DatabaseSource<'_>,
    owner: &ast::Struct,
    field: &ast::RecordField,
    generated: bool,
) -> bool {
    field.attrs().all(|attr| {
        named(&attr, "doc")
            || named(&attr, "cfg")
            || (generated && argument(&attr, "getter", "copy"))
            || (argument(&attr, "error", "ignore")
                && imports::has_derive(file, owner, "derive_more", "Error"))
    })
}

#[tracing::instrument(level = "trace", skip(file, owner, meta))]
pub(in crate::etiquettes::internal_error_chain::apply) fn stable_conditional(
    file: &DatabaseSource<'_>,
    owner: &ast::Struct,
    meta: &ast::Meta,
) -> bool {
    match meta {
        ast::Meta::CfgAttrMeta(meta) => {
            let nested: Vec<_> = meta.metas().collect();
            meta.cfg_predicate().is_some()
                && !nested.is_empty()
                && nested
                    .iter()
                    .all(|meta| stable_conditional(file, owner, meta))
        }
        ast::Meta::TokenTreeMeta(meta)
            if meta
                .path()
                .is_some_and(|path| path.syntax().text() == "display") =>
        {
            if !imports::has_display(file, owner) {
                return false;
            }
            let Some(args) = file.meta_expressions(meta) else {
                return false;
            };
            let Some(first) = args.first() else {
                return false;
            };
            if !matches!(first, ast::Expr::Literal(lit) if matches!(lit.kind(), ast::LiteralKind::String(_)))
            {
                return false;
            }
            args.iter().skip(1).all(|expr| {
                if let ast::Expr::PathExpr(path) = expr {
                    if path
                        .path()
                        .is_some_and(|path| path.syntax().text() == "file")
                    {
                        return true;
                    }
                }
                !expr
                    .syntax()
                    .descendants()
                    .any(|node| ast::MacroCall::can_cast(node.kind()))
                    && !expr
                        .syntax()
                        .descendants_with_tokens()
                        .filter_map(|part| part.into_token())
                        .any(|token| matches!(token.text(), "file" | "self" | "Self"))
            })
        }
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(attr))]
pub(super) fn doc_value(attr: &ast::Attr) -> Option<String> {
    if !named(attr, "doc") {
        return None;
    }
    let ast::Meta::KeyValueMeta(meta) = attr.meta()? else {
        return None;
    };
    let ast::Expr::Literal(literal) = meta.expr()? else {
        return None;
    };
    let ast::LiteralKind::String(value) = literal.kind() else {
        return None;
    };
    Some(value.value().ok()?.into_owned())
}
