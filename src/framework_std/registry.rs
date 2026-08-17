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

/// Features passed to `cargo run -p amenable -- dump-registry`.
pub const AMENABLE_DUMP_REGISTRY_FEATURES: &str = "creusot,verus";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegistryDump {
    pub evidence_links: Vec<EvidenceLinkDump>,
    pub proof_records: Vec<ProofRecordDump>,
    #[serde(default)]
    pub contract_records: Vec<ContractRecordDump>,
    pub kani_proofs: Vec<KaniProofDump>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceLinkDump {
    pub name: String,
    pub basis: String,
    pub index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofRecordDump {
    pub evidence: String,
    pub verifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KaniProofDump {
    pub id: String,
    pub harness: String,
    pub package: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractRecordDump {
    pub evidence: String,
    pub verifier: String,
    pub kind: String,
    pub fragment: String,
}

/// Run `cargo run -p amenable -- dump-registry` in the workspace.
#[instrument(skip(workspace, out_path), fields(out = %out_path.display()))]
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
pub fn load_registry_dump(path: &Path) -> CordialResult<RegistryDump> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

/// Extract the inventory-matching base type from a `RustStdStandard<…>` evidence name.
pub fn parse_rust_std_standard_inner(evidence: &str) -> Option<String> {
    let rest = evidence
        .strip_prefix(RUST_STD_STANDARD_PREFIX)
        .or_else(|| evidence.strip_prefix(PROOF_CHAIN_RUST_STD_PREFIX))?;
    let typed = extract_wrapped_type(rest)?;
    Some(type_path_without_generics(typed))
}

pub fn evidence_for_std_type(registry: &RegistryDump, type_path: &str) -> Option<String> {
    for link in &registry.evidence_links {
        let Some(inner) = parse_rust_std_standard_inner(&link.name) else {
            continue;
        };
        let singleton: HashSet<String> = HashSet::from([inner]);
        if type_has_trait_impl(&singleton, type_path) {
            return Some(link.name.clone());
        }
    }
    None
}

pub fn std_type_has_proof_test(proof_chain_subjects: &HashSet<String>, type_path: &str) -> bool {
    proof_chain_subjects
        .iter()
        .any(|subject| proof_chain_subject_matches_type(subject, type_path))
}

pub fn proof_chain_subject_matches_type(subject: &str, type_path: &str) -> bool {
    if let Some(inner) = parse_rust_std_standard_inner(subject) {
        let singleton: HashSet<String> = HashSet::from([inner]);
        return type_has_trait_impl(&singleton, type_path);
    }
    let singleton: HashSet<String> = HashSet::from([type_path_without_generics(subject)]);
    type_has_trait_impl(&singleton, type_path)
}

pub fn witness_verifiers_for_std_type(registry: &RegistryDump, type_path: &str) -> HashSet<String> {
    let mut verifiers = HashSet::new();
    for record in &registry.proof_records {
        let Some(inner) = parse_rust_std_standard_inner(&record.evidence) else {
            continue;
        };
        let singleton: HashSet<String> = HashSet::from([inner]);
        if type_has_trait_impl(&singleton, type_path) {
            verifiers.insert(record.verifier.clone());
        }
    }
    verifiers
}

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
