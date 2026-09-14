//! Parse `amenable dump-registry` JSON and match std inventory rows.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::framework_std::match_impl::{type_has_trait_impl, type_path_without_generics};

const RUST_STD_STANDARD_PREFIX: &str = "amenable_std::rust_std::RustStdStandard<";
const PROOF_CHAIN_RUST_STD_PREFIX: &str = "RustStdStandard<";
const RUST_STD_STANDARD_PREFIXES: &[&str] =
    &[RUST_STD_STANDARD_PREFIX, PROOF_CHAIN_RUST_STD_PREFIX];

/// `amenable_ext::ExtStandard<T>` — the third-party-crate counterpart of
/// `RustStdStandard<T>`. Two prefixes for the same reason
/// `RustStdStandard` needs a pair: the registry's own evidence/proof-
/// record names are fully qualified (`amenable_ext::ExtStandard<...>`),
/// but `collect_proof_chain_subjects` reads bare type names
/// (`ExtStandard<...>`, no module path) out of real `proof_chain_test.rs`
/// source text — a genuinely different naming convention, not a module-
/// nesting difference (confirmed by a real test failure this project
/// caught: `PROOF_CHAIN_EXT_STANDARD_PREFIX` was missing at first).
const EXT_STANDARD_PREFIX: &str = "amenable_ext::ExtStandard<";
const PROOF_CHAIN_EXT_STANDARD_PREFIX: &str = "ExtStandard<";
const EXT_STANDARD_PREFIXES: &[&str] = &[EXT_STANDARD_PREFIX, PROOF_CHAIN_EXT_STANDARD_PREFIX];

/// Features passed to `cargo run -p amenable -- dump-registry`.
pub const AMENABLE_DUMP_REGISTRY_FEATURES: &str = "creusot,verus";

/// Serializable dump of a std-family coverage registry.
#[derive(
    Debug,
    Clone,
    Default,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    derive_new::new,
    derive_getters::Getters,
)]
pub struct RegistryDump {
    /// Evidence links dumped from the registry.
    evidence_links: Vec<EvidenceLinkDump>,
    /// Proof records dumped from the registry.
    proof_records: Vec<ProofRecordDump>,
    /// Contract-bound records dumped from the registry.
    #[serde(default)]
    contract_records: Vec<ContractRecordDump>,
    /// Kani proof records dumped from the registry.
    kani_proofs: Vec<KaniProofDump>,
}

/// Serializable evidence link inside a registry dump.
#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Eq, derive_new::new, derive_getters::Getters,
)]
pub struct EvidenceLinkDump {
    /// Evidence item name.
    name: String,
    /// Evidence basis label.
    basis: String,
    /// Ordinal of this record in its list.
    #[getter(copy)]
    index: usize,
}

/// Serializable proof record inside a registry dump.
#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Eq, derive_new::new, derive_getters::Getters,
)]
pub struct ProofRecordDump {
    /// Supporting evidence paths or labels.
    evidence: String,
    /// Proof verifier this row is about (`kani`, `creusot`, …).
    verifier: String,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Eq, derive_new::new, derive_getters::Getters,
)]
pub struct KaniProofDump {
    id: String,
    harness: String,
    package: String,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Eq, derive_new::new, derive_getters::Getters,
)]
pub struct ContractRecordDump {
    evidence: String,
    verifier: String,
    kind: String,
    fragment: String,
}

/// Run `cargo run -p amenable -- dump-registry` in the workspace.
#[instrument(level = "info", skip(workspace), err(level = "warn"))]
pub fn run_amenable_dump_registry(workspace: &Path, out_path: &Path) -> CordialResult<()> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let status = Command::new("cargo")
        .current_dir(workspace)
        .arg("run")
        .arg("-p")
        .arg("amenable")
        .arg("--features")
        .arg(AMENABLE_DUMP_REGISTRY_FEATURES)
        .arg("--")
        .arg("dump-registry")
        .arg("--out")
        .arg(out_path)
        .status()
        .map_err(CordialError::from)?;

    if !status.success() {
        return Err(CordialError::invariant(format!(
            "amenable dump-registry exited with {status}"
        )));
    }
    if !out_path.is_file() {
        return Err(CordialError::invariant(format!(
            "registry dump not found at {}",
            out_path.display()
        )));
    }
    Ok(())
}

/// Load a registry dump from disk.
#[instrument(level = "info", skip(path), err(level = "warn"))]
pub fn load_registry_dump(path: &Path) -> CordialResult<RegistryDump> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

/// Extract the inventory-matching base type wrapped by one of `prefixes`
/// (`RustStdStandard<…>` or `ExtStandard<…>`) from an evidence name —
/// the one structural invariant [`parse_rust_std_standard_inner`] and
/// [`parse_ext_standard_inner`] share, generalized over which literal
/// prefix family each is stripping.
#[instrument(level = "debug")]
fn parse_wrapped_standard_inner(evidence: &str, prefixes: &[&str]) -> Option<String> {
    let rest = prefixes
        .iter()
        .find_map(|prefix| evidence.strip_prefix(prefix))?;
    let typed = extract_wrapped_type(rest)?;
    Some(type_path_without_generics(typed))
}

/// Extract the inventory-matching base type from a `RustStdStandard<…>` evidence name.
#[instrument(level = "debug")]
pub fn parse_rust_std_standard_inner(evidence: &str) -> Option<String> {
    parse_wrapped_standard_inner(evidence, RUST_STD_STANDARD_PREFIXES)
}

/// Extract the inventory-matching base type from an `ExtStandard<…>` evidence name.
#[instrument(level = "debug")]
pub fn parse_ext_standard_inner(evidence: &str) -> Option<String> {
    parse_wrapped_standard_inner(evidence, EXT_STANDARD_PREFIXES)
}

/// Evidence for a type wrapped by `parse_inner`'s prefix family — shared
/// by [`evidence_for_std_type`] and [`evidence_for_ext_type`].
#[instrument(level = "debug", skip(registry, parse_inner))]
fn evidence_for_wrapped_type(
    registry: &RegistryDump,
    type_path: &str,
    parse_inner: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    for link in registry.evidence_links() {
        let Some(inner) = parse_inner(&link.name) else {
            continue;
        };
        let singleton: HashSet<String> = HashSet::from([inner]);
        if type_has_trait_impl(&singleton, type_path) {
            return Some(link.name.clone());
        }
    }
    None
}

/// Evidence for std type.
#[instrument(level = "debug", skip(registry))]
pub fn evidence_for_std_type(registry: &RegistryDump, type_path: &str) -> Option<String> {
    evidence_for_wrapped_type(registry, type_path, parse_rust_std_standard_inner)
}

/// Evidence for a third-party (`amenable_ext`) type.
#[instrument(level = "debug", skip(registry))]
pub fn evidence_for_ext_type(registry: &RegistryDump, type_path: &str) -> Option<String> {
    evidence_for_wrapped_type(registry, type_path, parse_ext_standard_inner)
}

/// Whether any proof-chain subject wrapped by `parse_inner`'s prefix
/// family (or bare) matches `type_path` — shared by
/// [`std_type_has_proof_test`] and [`ext_type_has_proof_test`].
#[instrument(level = "debug", skip(proof_chain_subjects, parse_inner))]
fn type_has_proof_test(
    proof_chain_subjects: &HashSet<String>,
    type_path: &str,
    parse_inner: impl Fn(&str) -> Option<String> + Copy,
) -> bool {
    proof_chain_subjects
        .iter()
        .any(|subject| proof_chain_subject_matches_wrapped_type(subject, type_path, parse_inner))
}

#[instrument(level = "debug", skip(proof_chain_subjects))]
pub fn std_type_has_proof_test(proof_chain_subjects: &HashSet<String>, type_path: &str) -> bool {
    type_has_proof_test(
        proof_chain_subjects,
        type_path,
        parse_rust_std_standard_inner,
    )
}

/// As [`std_type_has_proof_test`], for a third-party (`amenable_ext`) type.
#[instrument(level = "debug", skip(proof_chain_subjects))]
pub fn ext_type_has_proof_test(proof_chain_subjects: &HashSet<String>, type_path: &str) -> bool {
    type_has_proof_test(proof_chain_subjects, type_path, parse_ext_standard_inner)
}

#[instrument(level = "debug", skip(parse_inner))]
fn proof_chain_subject_matches_wrapped_type(
    subject: &str,
    type_path: &str,
    parse_inner: impl Fn(&str) -> Option<String>,
) -> bool {
    if let Some(inner) = parse_inner(subject) {
        let singleton: HashSet<String> = HashSet::from([inner]);
        return type_has_trait_impl(&singleton, type_path);
    }
    let singleton: HashSet<String> = HashSet::from([type_path_without_generics(subject)]);
    type_has_trait_impl(&singleton, type_path)
}

/// Witness verifiers for a type wrapped by `parse_inner`'s prefix family
/// — shared by [`witness_verifiers_for_std_type`] and
/// [`witness_verifiers_for_ext_type`].
#[instrument(level = "debug", skip(registry, parse_inner))]
fn witness_verifiers_for_wrapped_type(
    registry: &RegistryDump,
    type_path: &str,
    parse_inner: impl Fn(&str) -> Option<String>,
) -> HashSet<String> {
    let mut verifiers = HashSet::new();
    for record in registry.proof_records() {
        let Some(inner) = parse_inner(&record.evidence) else {
            continue;
        };
        let singleton: HashSet<String> = HashSet::from([inner]);
        if type_has_trait_impl(&singleton, type_path) {
            verifiers.insert(record.verifier.clone());
        }
    }
    verifiers
}

/// Witness verifiers for std type.
#[instrument(level = "debug", skip(registry))]
pub fn witness_verifiers_for_std_type(registry: &RegistryDump, type_path: &str) -> HashSet<String> {
    witness_verifiers_for_wrapped_type(registry, type_path, parse_rust_std_standard_inner)
}

/// Witness verifiers for a third-party (`amenable_ext`) type.
#[instrument(level = "debug", skip(registry))]
pub fn witness_verifiers_for_ext_type(registry: &RegistryDump, type_path: &str) -> HashSet<String> {
    witness_verifiers_for_wrapped_type(registry, type_path, parse_ext_standard_inner)
}

#[instrument(level = "debug")]
fn extract_wrapped_type(rest: &str) -> Option<&str> {
    let mut depth = 0i32;
    for (index, ch) in rest.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth < 0 {
                    return Some(&rest[..index]);
                }
            }
            _ => {}
        }
    }
    None
}
