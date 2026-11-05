//! Registered contract index and named-call recognition.

use std::collections::HashMap;

use tracing::instrument;

use super::super::facts::{ContractCall, ContractClause};
use super::dump::ContractRecordDump;
use super::fragments::{fragment_fn_body_text, fragment_fn_name};
use super::tokens::{canonicalize_type_text, normalize_text};

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

    /// Match source-derived shapes; source syntax is never reconstructed here.
    #[instrument(level = "debug", skip(self, clause))]
    pub(in crate::etiquettes::antipatterns::contract_bounds) fn matches_clause(
        &self,
        clause: &ContractClause,
    ) -> bool {
        let verifier = clause.verifier();
        let kind = clause.kind();
        let Some(known) = self.records.get(&(verifier.clone(), kind.clone())) else {
            return false;
        };
        let named = match clause.call() {
            ContractCall::None => false,
            ContractCall::Bare(name) => known
                .iter()
                .any(|(_, fragment)| fragment_fn_name(fragment).as_ref() == Some(name)),
            ContractCall::Typed { predicate, prefix } => {
                predicate == kind
                    && known
                        .iter()
                        .any(|(evidence, _)| normalize_text(evidence).ends_with(prefix))
            }
        };
        named
            || clause.sibling_names().iter().any(|name| {
                self.named_fragment_body(verifier, kind, name)
                    .is_some_and(|body| canonicalize_type_text(&body) == *clause.canonical_body())
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
