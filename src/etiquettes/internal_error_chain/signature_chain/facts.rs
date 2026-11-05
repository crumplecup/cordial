//! Closed signature grammar retained in shared IR, independent of parser nodes.

use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub(super) struct Path {
    parts: Vec<String>,
    args: Option<Vec<Type>>,
    #[getter(copy)]
    simple: bool,
}

impl Path {
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn ident(&self) -> Option<&str> {
        self.simple
            .then(|| self.parts.first().map(String::as_str))
            .flatten()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Type {
    Path(Path),
    Transparent(Box<Type>),
    Tuple(Vec<Type>),
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Expr {
    Path(Path),
    Call { path: Option<Path>, args: Vec<Expr> },
    Transparent(Box<Expr>),
    Unsupported,
}

#[tracing::instrument(level = "trace", skip(path))]
pub(super) fn lower_path(path: &syn::Path) -> Path {
    Path {
        parts: path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect(),
        args: path.segments.last().and_then(|part| match &part.arguments {
            syn::PathArguments::None => Some(Vec::new()),
            syn::PathArguments::AngleBracketed(list) => Some(
                list.args
                    .iter()
                    .filter_map(|arg| match arg {
                        syn::GenericArgument::Type(ty) => Some(lower_type(ty)),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        }),
        simple: path.get_ident().is_some(),
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn lower_type(ty: &syn::Type) -> Type {
    match ty {
        syn::Type::Path(path) if path.qself.is_none() => Type::Path(lower_path(&path.path)),
        syn::Type::Reference(reference) => Type::Transparent(Box::new(lower_type(&reference.elem))),
        syn::Type::Paren(paren) => Type::Transparent(Box::new(lower_type(&paren.elem))),
        syn::Type::Group(group) => Type::Transparent(Box::new(lower_type(&group.elem))),
        syn::Type::Tuple(tuple) => Type::Tuple(tuple.elems.iter().map(lower_type).collect()),
        _ => Type::Unsupported,
    }
}

#[tracing::instrument(level = "trace", skip(expr))]
pub(super) fn lower_expr(expr: &syn::Expr) -> Expr {
    match expr {
        syn::Expr::Path(path) => Expr::Path(lower_path(&path.path)),
        syn::Expr::Call(call) => Expr::Call {
            path: match call.func.as_ref() {
                syn::Expr::Path(path) => Some(lower_path(&path.path)),
                _ => None,
            },
            args: call.args.iter().map(lower_expr).collect(),
        },
        syn::Expr::Block(block) => match block.block.stmts.last() {
            Some(syn::Stmt::Expr(tail, _)) => Expr::Transparent(Box::new(lower_expr(tail))),
            _ => Expr::Unsupported,
        },
        syn::Expr::Paren(paren) => Expr::Transparent(Box::new(lower_expr(&paren.expr))),
        syn::Expr::Group(group) => Expr::Transparent(Box::new(lower_expr(&group.expr))),
        _ => Expr::Unsupported,
    }
}
