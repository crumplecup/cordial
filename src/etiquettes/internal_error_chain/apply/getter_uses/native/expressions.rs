//! Lexical receiver provenance, without resolving crate declarations here.
use super::{Collector, ReceiverRecipe, UseKind, contracts};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasName},
};
use std::collections::BTreeMap;
use tracing::instrument;

impl Collector<'_, '_> {
    #[instrument(level = "trace", skip(self, ty))]
    pub(super) fn ty(&self, ty: &ast::Type) -> ReceiverRecipe {
        if self.scope.local_types {
            return ReceiverRecipe::Unknown;
        }
        if let ast::Type::PathType(value) = ty
            && value
                .path()
                .is_some_and(|path| bare(&path).as_deref() == Some("Self"))
        {
            return self.scope.owner.clone();
        }
        contracts::ty_recipe(self.file, ty)
    }

    #[instrument(level = "trace", skip(self, expr))]
    pub(super) fn receiver(&self, expr: &ast::Expr) -> ReceiverRecipe {
        match expr {
            ast::Expr::PathExpr(value) => value
                .path()
                .and_then(|path| bare(&path))
                .and_then(|name| self.scope.bindings.get(&name))
                .cloned()
                .unwrap_or(ReceiverRecipe::Unknown),
            ast::Expr::RefExpr(value) => value
                .expr()
                .map_or(ReceiverRecipe::Unknown, |expr| self.receiver(&expr)),
            ast::Expr::ParenExpr(value) => value
                .expr()
                .map_or(ReceiverRecipe::Unknown, |expr| self.receiver(&expr)),
            ast::Expr::CallExpr(value) => {
                let Some(ast::Expr::PathExpr(callee)) = value.expr() else {
                    return ReceiverRecipe::Unknown;
                };
                let Some(path) = callee.path() else {
                    return ReceiverRecipe::Unknown;
                };
                if let Some(name) = bare(&path) {
                    if self.scope.bindings.contains_key(&name) {
                        return ReceiverRecipe::Unknown;
                    }
                    return self
                        .scope
                        .functions
                        .get(&name)
                        .cloned()
                        .unwrap_or(ReceiverRecipe::Unknown);
                }
                if path
                    .segment()
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|name| name.text() == "caller")
                    && value
                        .arg_list()
                        .is_some_and(|args| args.args().next().is_none())
                    && !self.scope.local_types
                    && path.qualifier().is_some_and(|path| {
                        contracts::path_recipe(self.file, &path) == ReceiverRecipe::Location
                    })
                {
                    ReceiverRecipe::Location
                } else {
                    ReceiverRecipe::Unknown
                }
            }
            _ => ReceiverRecipe::Unknown,
        }
    }

    #[instrument(level = "trace", skip(self, node))]
    pub(super) fn expression(&mut self, node: &SyntaxNode, offset: usize) {
        let fact = if let Some(expr) = ast::FieldExpr::cast(node.clone()) {
            expr.name_ref()
                .filter(|name| name.text() == "file")
                .map(|_| {
                    (
                        expr.expr()
                            .map_or(ReceiverRecipe::Unknown, |expr| self.receiver(&expr)),
                        UseKind::Field,
                    )
                })
        } else if let Some(expr) = ast::MethodCallExpr::cast(node.clone()) {
            expr.name_ref()
                .filter(|name| name.text() == "file")
                .map(|_| {
                    (
                        expr.receiver()
                            .map_or(ReceiverRecipe::Unknown, |expr| self.receiver(&expr))
                            .method(false),
                        UseKind::Getter,
                    )
                })
        } else if let Some(expr) = ast::PathExpr::cast(node.clone()) {
            expr.path().and_then(|path| {
                if !path
                    .segment()
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|name| name.text() == "file")
                {
                    return None;
                }
                let owner = path.qualifier()?;
                let receiver = if self.scope.local_types || contracts::path_parts(&path).is_none() {
                    ReceiverRecipe::Unknown
                } else if bare(&owner).as_deref() == Some("Self") {
                    self.scope.owner.clone()
                } else {
                    contracts::path_recipe(self.file, &owner)
                };
                Some((receiver.method(true), UseKind::Getter))
            })
        } else {
            None
        };
        if let Some((receiver, kind)) = fact {
            let range = node.text_range();
            self.record(
                offset + usize::from(range.start()),
                offset + usize::from(range.end()),
                receiver,
                kind,
            );
        }
    }
}

#[instrument(level = "trace", skip(path))]
fn bare(path: &ast::Path) -> Option<String> {
    let (names, arguments) = contracts::path_parts(path)?;
    if arguments
        || path.coloncolon_token().is_some()
        || path.segment()?.coloncolon_token().is_some()
    {
        return None;
    }
    match names.as_slice() {
        [name] => Some(name.clone()),
        _ => None,
    }
}

#[instrument(level = "trace", skip(bindings, pat, value))]
pub(super) fn bind(
    bindings: &mut BTreeMap<String, ReceiverRecipe>,
    pat: &ast::Pat,
    value: ReceiverRecipe,
) {
    match pat {
        ast::Pat::IdentPat(pat) => {
            if let Some(name) = pat.name() {
                bindings.insert(
                    name.text().to_string(),
                    if pat.mut_token().is_none() && pat.ref_token().is_none() && pat.pat().is_none()
                    {
                        value
                    } else {
                        ReceiverRecipe::Unknown
                    },
                );
            }
        }
        ast::Pat::TuplePat(pat) => {
            for pat in pat.fields() {
                bind(bindings, &pat, ReceiverRecipe::Unknown);
            }
        }
        _ => {}
    }
}
