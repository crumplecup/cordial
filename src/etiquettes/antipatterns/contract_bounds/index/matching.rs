//! Registered contract index and named-call recognition.

use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::ToTokens;
use tracing::instrument;

use super::dump::ContractRecordDump;
use super::fragments::{fragment_fn_body_text, fragment_fn_name};
use super::tokens::{
    bare_named_call_name, canonicalize_type_text, named_call_name_allowing_leading_attr,
    normalize_text, normalize_tokens, strip_trigger_attrs, strip_turbofish,
};

/// Registered contract records indexed by `(verifier, kind)` for lookup --
/// each entry keeps both `evidence` (the contract type's name, for Kani's
/// type-prefix suffix match) and `fragment` (the predicate's own source
/// text, for Creusot/Verus's callable-name extraction).
pub(in crate::etiquettes::antipatterns::contract_bounds) struct ContractIndex {
    records: ContractRecordMap,
}

/// `(verifier, kind) -> [(evidence, fragment)]`.
type ContractRecordMap = HashMap<(String, String), Vec<(String, String)>>;

impl ContractIndex {
    #[instrument(level = "debug", skip(records))]
    pub(in crate::etiquettes::antipatterns::contract_bounds) fn build(
        records: &[ContractRecordDump],
    ) -> Self {
        let mut by_key: ContractRecordMap = HashMap::new();
        for record in records {
            by_key
                .entry((record.verifier().clone(), record.kind().clone()))
                .or_default()
                .push((record.evidence().clone(), record.fragment().clone()));
        }
        Self { records: by_key }
    }

    /// Whether `clause` is a real call to some registered contract's
    /// predicate -- not a text-equality check, a call-shape recognition
    /// check. Kani uses typed-path calls such as `Type::ensures(...)`;
    /// Creusot and Verus use bare predicate calls such as `name(...)`.
    #[instrument(level = "debug", skip(self, clause))]
    pub(in crate::etiquettes::antipatterns::contract_bounds) fn matches_named_call(
        &self,
        verifier: &str,
        kind: &str,
        clause: TokenStream,
    ) -> bool {
        let Some(known) = self.records.get(&(verifier.to_string(), kind.to_string())) else {
            return false;
        };

        if verifier != "kani"
            && let Some(name) = bare_named_call_name(clause.clone())
        {
            return known
                .iter()
                .any(|(_, fragment)| fragment_fn_name(fragment).as_deref() == Some(name.as_str()));
        }

        let Ok(expr) = syn::parse2::<syn::Expr>(clause) else {
            return false;
        };
        let call = match &expr {
            syn::Expr::Call(call) => call,
            syn::Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Not(_)) => {
                match unary.expr.as_ref() {
                    syn::Expr::Call(call) => call,
                    _ => return false,
                }
            }
            _ => return false,
        };
        let syn::Expr::Path(func_path) = call.func.as_ref() else {
            return false;
        };
        let segments = &func_path.path.segments;
        let Some(last) = segments.last() else {
            return false;
        };

        if last.ident == kind {
            if let Some(qself) = &func_path.qself {
                let prefix_text =
                    canonicalize_type_text(&normalize_tokens(qself.ty.to_token_stream()));
                return known
                    .iter()
                    .any(|(evidence, _)| normalize_text(evidence).ends_with(&prefix_text));
            }

            if segments.len() >= 2 {
                let mut prefix = syn::Path {
                    leading_colon: func_path.path.leading_colon,
                    segments: syn::punctuated::Punctuated::new(),
                };
                for seg in segments.iter().take(segments.len() - 1) {
                    prefix.segments.push(seg.clone());
                }
                let prefix_text = canonicalize_type_text(&strip_turbofish(&normalize_tokens(
                    prefix.to_token_stream(),
                )));
                return known
                    .iter()
                    .any(|(evidence, _)| normalize_text(evidence).ends_with(&prefix_text));
            }
        }

        if segments.len() == 1 && func_path.qself.is_none() {
            let name = last.ident.to_string();
            return known
                .iter()
                .any(|(_, fragment)| fragment_fn_name(fragment).as_deref() == Some(name.as_str()));
        }

        false
    }

    /// Whether `clause` is a raw restatement of a named sibling clause
    /// from the same requires/ensures list.
    #[instrument(level = "trace", skip(self, clause, siblings))]
    pub(in crate::etiquettes::antipatterns::contract_bounds) fn is_raw_duplicate_of_named_sibling(
        &self,
        verifier: &str,
        kind: &str,
        clause: TokenStream,
        siblings: &[TokenStream],
        idx: usize,
    ) -> bool {
        let own_normalized = canonicalize_type_text(&normalize_tokens(strip_trigger_attrs(clause)));
        siblings.iter().enumerate().any(|(sibling_idx, sibling)| {
            sibling_idx != idx
                && named_call_name_allowing_leading_attr(sibling.clone()).is_some_and(|name| {
                    self.named_fragment_body(verifier, kind, &name)
                        .is_some_and(|body| canonicalize_type_text(&body) == own_normalized)
                })
        })
    }

    /// The normalized body text of the registered `(verifier, kind)`
    /// fragment whose own `fn` name is `name`, if any.
    #[instrument(level = "trace", skip(self))]
    fn named_fragment_body(&self, verifier: &str, kind: &str, name: &str) -> Option<String> {
        let known = self
            .records
            .get(&(verifier.to_string(), kind.to_string()))?;
        known.iter().find_map(|(_, fragment)| {
            (fragment_fn_name(fragment).as_deref() == Some(name))
                .then(|| fragment_fn_body_text(fragment))
                .flatten()
        })
    }
}
