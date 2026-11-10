//! Source-derived contract call shapes retained independently of registry policy.

use super::index::tokens::{
    bare_named_call_name, canonicalize_type_text, expression, named_call_name_allowing_leading_attr,
    strip_trigger_attrs, strip_turbofish,
};
use super::index::ContractIndex;
use super::token_tree::{Token, render};
use crate::etiquettes::antipatterns::types::AntipatternSiteRecord;
use crate::loader::database_token_text as token_text;
use ra_ap_syntax::{AstNode, ast};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub(crate) struct ContractClause {
    site: AntipatternSiteRecord,
    verifier: String,
    kind: String,
    call: ContractCall,
    canonical_body: String,
    sibling_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ContractCall {
    None,
    Bare(String),
    Typed { predicate: String, prefix: String },
}

impl ContractClause {
    #[tracing::instrument(level = "debug", skip_all)]
    pub(super) fn collect(
        site: AntipatternSiteRecord,
        verifier: &str,
        kind: &str,
        tokens: Vec<Token>,
        sibling_names: Vec<String>,
    ) -> Self {
        Self {
            site,
            verifier: verifier.into(),
            kind: kind.into(),
            call: ContractCall::collect(verifier, &tokens),
            canonical_body: canonicalize_type_text(&render(&strip_trigger_attrs(tokens))),
            sibling_names,
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub(super) fn sibling_name(tokens: &[Token]) -> Option<String> {
        named_call_name_allowing_leading_attr(tokens)
    }

    #[tracing::instrument(level = "trace", skip(self, index))]
    pub(super) fn finding(&self, index: &ContractIndex) -> Option<AntipatternSiteRecord> {
        (!index.matches_clause(self)).then(|| self.site.clone())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn strip_file_prefix(&mut self, root: &std::path::Path) {
        self.site.strip_file_prefix(root);
    }
}

impl ContractCall {
    #[tracing::instrument(level = "debug", skip(clause))]
    fn collect(verifier: &str, clause: &[Token]) -> Self {
        if verifier != "kani"
            && let Some(name) = bare_named_call_name(clause)
        {
            return Self::Bare(name);
        }
        expression(clause).map_or(Self::None, |expr| Self::from_expr(&expr))
    }

    /// The call a whole clause expression makes: `name(..)`, `!name(..)`, or
    /// `Type::name(..)`, with the type prefix canonicalized for matching.
    #[tracing::instrument(level = "trace", skip(expr))]
    pub(super) fn from_expr(expr: &ast::Expr) -> Self {
        let call = match expr {
            ast::Expr::CallExpr(call) => Some(call.clone()),
            ast::Expr::PrefixExpr(prefix) if prefix.op_kind() == Some(ast::UnaryOp::Not) => {
                prefix.expr().and_then(|expr| match expr {
                    ast::Expr::CallExpr(call) => Some(call),
                    _ => None,
                })
            }
            _ => None,
        };
        let Some(ast::Expr::PathExpr(function)) = call.and_then(|call| call.expr()) else {
            return Self::None;
        };
        let Some(path) = function.path() else {
            return Self::None;
        };
        let Some(predicate) = path
            .segment()
            .and_then(|segment| segment.name_ref())
            .map(|name| name.text().to_string())
        else {
            return Self::None;
        };
        let mut first = path.clone();
        while let Some(qualifier) = first.qualifier() {
            first = qualifier;
        }
        if let Some(ty) = first
            .segment()
            .and_then(|segment| segment.type_anchor())
            .and_then(|anchor| anchor.ty())
        {
            return Self::Typed {
                predicate,
                prefix: canonicalize_type_text(&token_text(ty.syntax())),
            };
        }
        match path.qualifier() {
            None => Self::Bare(predicate),
            Some(prefix) => Self::Typed {
                predicate,
                prefix: canonicalize_type_text(&strip_turbofish(&token_text(prefix.syntax()))),
            },
        }
    }
}
