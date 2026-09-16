//! Registered contract fragments and call-shape matching.

mod clauses;
mod dump;
mod fragments;
mod matching;
mod tokens;

pub(super) use clauses::{is_builtin_contract_inspection, is_trivial};
pub(super) use dump::RegistryDump;
pub use dump::{ContractRecordDump, ContractRecordDumpBuilder};
pub(super) use matching::ContractIndex;
pub(super) use tokens::{normalize_tokens, split_top_level_commas};

/// Which verifier a crate name maps to, if any -- the only crates this rule
/// applies to.
#[tracing::instrument(level = "debug")]
pub(super) fn verifier_for_crate(crate_name: &str) -> Option<&'static str> {
    match crate_name {
        "amenable_creusot" => Some("creusot"),
        "amenable_verus" => Some("verus"),
        "amenable_kani" => Some("kani"),
        _ => None,
    }
}
