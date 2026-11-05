//! Source-derived contract call shapes retained independently of registry policy.

use super::index::tokens::{
    bare_named_call_name, canonicalize_type_text, named_call_name_allowing_leading_attr,
    strip_trigger_attrs, strip_turbofish,
};
use super::index::{ContractIndex, normalize_tokens};
use crate::etiquettes::antipatterns::types::AntipatternSiteRecord;
use proc_macro2::TokenStream;
use quote::ToTokens;
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
        tokens: TokenStream,
        sibling_names: Vec<String>,
    ) -> Self {
        Self {
            site,
            verifier: verifier.into(),
            kind: kind.into(),
            call: ContractCall::collect(verifier, tokens.clone()),
            canonical_body: canonicalize_type_text(&normalize_tokens(strip_trigger_attrs(tokens))),
            sibling_names,
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub(super) fn sibling_name(tokens: TokenStream) -> Option<String> {
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
    fn collect(verifier: &str, clause: TokenStream) -> Self {
        if verifier != "kani"
            && let Some(name) = bare_named_call_name(clause.clone())
        {
            return Self::Bare(name);
        }
        let Ok(expr) = syn::parse2::<syn::Expr>(clause) else {
            return Self::None;
        };
        let call = match &expr {
            syn::Expr::Call(call) => call,
            syn::Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Not(_)) => {
                let syn::Expr::Call(call) = unary.expr.as_ref() else {
                    return Self::None;
                };
                call
            }
            _ => return Self::None,
        };
        let syn::Expr::Path(func) = call.func.as_ref() else {
            return Self::None;
        };
        let Some(last) = func.path.segments.last() else {
            return Self::None;
        };
        if let Some(qself) = &func.qself {
            return Self::Typed {
                predicate: last.ident.to_string(),
                prefix: canonicalize_type_text(&normalize_tokens(qself.ty.to_token_stream())),
            };
        }
        if func.path.segments.len() == 1 {
            return Self::Bare(last.ident.to_string());
        }
        let mut prefix = func.path.clone();
        prefix.segments.pop();
        prefix.segments.pop_punct();
        Self::Typed {
            predicate: last.ident.to_string(),
            prefix: canonicalize_type_text(&strip_turbofish(&normalize_tokens(
                prefix.to_token_stream(),
            ))),
        }
    }
}
