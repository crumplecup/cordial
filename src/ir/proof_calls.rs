//! Raw proof reachability evidence collected once with workspace source facts.

use serde::{Deserialize, Serialize};

mod compatibility;
mod native;

/// One function's names, inherited proof markers and direct path-call keys.
/// These are syntax facts, not resolved semantic edges or a policy decision.
/// Required fields distinguish complete empty evidence from a corrupt payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceProofFunction {
    qualified_name: String,
    call_keys: Vec<String>,
    cfg_names: Vec<String>,
    attribute_names: Vec<String>,
    calls: Vec<String>,
}

#[tracing::instrument(level = "trace")]
fn qualify(prefix: &[String], local: &str) -> String {
    prefix
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(local))
        .collect::<Vec<_>>()
        .join("::")
}

#[tracing::instrument(level = "trace")]
fn assert_arguments(name: &str) -> usize {
    match name {
        "assert" | "debug_assert" => 1,
        "assert_eq" | "assert_ne" | "debug_assert_eq" | "debug_assert_ne" => 2,
        _ => 0,
    }
}
