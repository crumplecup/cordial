//! Amenable std registry coverage — std inventory vs Provenance/Witness layers.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::framework_std::StdInventoryItem;
use crate::framework_std::registry::{
    RegistryDump, evidence_for_std_type, std_type_has_proof_test, witness_verifiers_for_std_type,
};
use crate::framework_std::types::framework_std_type_items;
use crate::framework_std::verifier_skip::VerifierSkipMap;

/// Overall registration status for one std type row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmenableStdStatus {
    Complete,
    Partial,
    Missing,
    Skipped,
}

impl std::fmt::Display for AmenableStdStatus {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Complete => write!(f, "Complete"),
            Self::Partial => write!(f, "Partial"),
            Self::Missing => write!(f, "Missing"),
            Self::Skipped => write!(f, "Skipped"),
        }
    }
}

/// One row in an amenable std registry coverage report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AmenableStdEntry {
    pub type_path: String,
    pub type_kind: String,
    pub is_generic: bool,
    pub evidence_link: bool,
    pub evidence_name: Option<String>,
    pub kani_witness: bool,
    pub creusot_witness: bool,
    pub verus_witness: bool,
    pub proof_test: bool,
    pub status: AmenableStdStatus,
    pub skip_reason: Option<String>,
    #[serde(default)]
    pub kani_excepted: bool,
    #[serde(default)]
    pub creusot_excepted: bool,
    #[serde(default)]
    pub verus_excepted: bool,
}

/// Coverage report for amenable std registry vs std type inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmenableStdReport {
    pub source_crate: String,
    pub impl_crate: String,
    pub include_nightly: bool,
    pub entries: Vec<AmenableStdEntry>,
    pub complete_count: usize,
    pub partial_count: usize,
    pub missing_count: usize,
    pub skipped_count: usize,
}

impl AmenableStdReport {
    #[instrument(level = "debug", skip(self))]
    pub fn coverage_pct(&self) -> f32 {
        let accountable = self.entries.len().saturating_sub(self.skipped_count);
        if accountable == 0 {
            0.0
        } else {
            self.complete_count as f32 / accountable as f32 * 100.0
        }
    }
}

/// One actionable gap row for amenable std registry coverage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmenableStdGapEntry {
    pub source_crate: String,
    pub type_path: String,
    pub type_kind: String,
    pub status: AmenableStdStatus,
    pub missing_layers: String,
    pub action: String,
}

/// Build an amenable std registry coverage report.
#[instrument(level = "debug", skip(items, registry, skip_map, proof_chain_subjects))]
pub fn build_amenable_std_report(
    source_crate: &str,
    items: &[StdInventoryItem],
    impl_crate: &str,
    registry: &RegistryDump,
    skip_map: &VerifierSkipMap,
    proof_chain_subjects: &HashSet<String>,
    include_nightly: bool,
) -> AmenableStdReport {
    let mut entries = Vec::new();
    let mut complete_count = 0usize;
    let mut partial_count = 0usize;
    let mut missing_count = 0usize;
    let mut skipped_count = 0usize;

    for item in framework_std_type_items(items, include_nightly) {
        let entry = classify_amenable_std_row(
            &item.path,
            item.kind.as_str(),
            item.is_generic,
            item.alias_target.as_deref(),
            items,
            registry,
            skip_map,
            proof_chain_subjects,
        );
        match entry.status {
            AmenableStdStatus::Complete => complete_count += 1,
            AmenableStdStatus::Partial => partial_count += 1,
            AmenableStdStatus::Missing => missing_count += 1,
            AmenableStdStatus::Skipped => skipped_count += 1,
        }
        entries.push(entry);
    }

    AmenableStdReport {
        source_crate: source_crate.to_string(),
        impl_crate: impl_crate.to_string(),
        include_nightly,
        entries,
        complete_count,
        partial_count,
        missing_count,
        skipped_count,
    }
}

/// Classify one std inventory row for amenable registry coverage.
#[instrument(level = "debug", skip(items, registry, skip_map, proof_chain_subjects))]
pub fn classify_amenable_std_row(
    type_path: &str,
    type_kind: &str,
    is_generic: bool,
    alias_target: Option<&str>,
    items: &[StdInventoryItem],
    registry: &RegistryDump,
    skip_map: &VerifierSkipMap,
    proof_chain_subjects: &HashSet<String>,
) -> AmenableStdEntry {
    let exception = skip_map.get(type_path);

    if let Some(exception) = exception
        && exception.verifiers.is_none()
    {
        return AmenableStdEntry {
            type_path: type_path.to_string(),
            type_kind: type_kind.to_string(),
            is_generic,
            evidence_link: false,
            evidence_name: None,
            kani_witness: false,
            creusot_witness: false,
            verus_witness: false,
            proof_test: false,
            status: AmenableStdStatus::Skipped,
            skip_reason: Some(exception.reason.clone()),
            kani_excepted: true,
            creusot_excepted: true,
            verus_excepted: true,
        };
    }

    let mut evidence_name = evidence_for_std_type(registry, type_path);
    let mut verifiers = witness_verifiers_for_std_type(registry, type_path);
    let mut proof_test = std_type_has_proof_test(proof_chain_subjects, type_path);
    if evidence_name.is_none()
        && let Some(target) = alias_target
    {
        let resolved_target = resolve_alias_chain(items, target, 5);
        evidence_name = evidence_for_std_type(registry, &resolved_target);
        verifiers = witness_verifiers_for_std_type(registry, &resolved_target);
        proof_test = std_type_has_proof_test(proof_chain_subjects, &resolved_target);
    }
    let evidence_link = evidence_name.is_some();
    let kani_witness = verifiers.contains("kani");
    let creusot_witness = verifiers.contains("creusot");
    let verus_witness = verifiers.contains("verus");

    let kani_applicable = exception.is_none_or(|e| !e.covers("kani"));
    let creusot_applicable = exception.is_none_or(|e| !e.covers("creusot"));
    let verus_applicable = exception.is_none_or(|e| !e.covers("verus"));

    let status = if !evidence_link {
        AmenableStdStatus::Missing
    } else if (!kani_applicable || kani_witness)
        && (!creusot_applicable || creusot_witness)
        && (!verus_applicable || verus_witness)
    {
        AmenableStdStatus::Complete
    } else {
        AmenableStdStatus::Partial
    };

    AmenableStdEntry {
        type_path: type_path.to_string(),
        type_kind: type_kind.to_string(),
        is_generic,
        evidence_link,
        evidence_name,
        kani_witness,
        creusot_witness,
        verus_witness,
        proof_test,
        status,
        skip_reason: exception.map(|e| e.reason.clone()),
        kani_excepted: !kani_applicable,
        creusot_excepted: !creusot_applicable,
        verus_excepted: !verus_applicable,
    }
}

/// Gap metadata for one amenable std row.
#[instrument(level = "debug", skip(entry))]
pub fn amenable_gap_fields(entry: &AmenableStdEntry, impl_crate: &str) -> (String, String) {
    (
        missing_layer_labels(entry).join(", "),
        gap_action(entry, impl_crate),
    )
}

#[instrument(level = "debug", skip(items))]
pub fn resolve_alias_chain(
    items: &[StdInventoryItem],
    start: &str,
    max_hops: usize,
) -> String {
    let mut current = start.to_string();
    for _ in 0..max_hops {
        let Some(next_item) = items.iter().find(|candidate| {
            candidate.path == current || candidate.path.ends_with(&format!("::{current}"))
        }) else {
            break;
        };
        let Some(next_target) = &next_item.alias_target else {
            break;
        };
        if *next_target == current {
            break;
        }
        current = next_target.clone();
    }
    current
}

/// Build consolidated gap rows from an amenable std report.
#[instrument(level = "debug", skip(report))]
pub fn build_amenable_std_gaps(report: &AmenableStdReport) -> Vec<AmenableStdGapEntry> {
    report
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.status,
                AmenableStdStatus::Missing | AmenableStdStatus::Partial
            )
        })
        .map(|entry| {
            let (missing_layers, action) = amenable_gap_fields(entry, &report.impl_crate);
            AmenableStdGapEntry {
                source_crate: report.source_crate.clone(),
                type_path: entry.type_path.clone(),
                type_kind: entry.type_kind.clone(),
                status: entry.status,
                missing_layers,
                action,
            }
        })
        .collect()
}

#[instrument(level = "debug", skip(entry))]
fn missing_layer_labels(entry: &AmenableStdEntry) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if !entry.evidence_link {
        missing.push("evidence_link");
    }
    if entry.evidence_link && !entry.kani_witness && !entry.kani_excepted {
        missing.push("kani_witness");
    }
    if entry.evidence_link && !entry.creusot_witness && !entry.creusot_excepted {
        missing.push("creusot_witness");
    }
    if entry.evidence_link && !entry.verus_witness && !entry.verus_excepted {
        missing.push("verus_witness");
    }
    if !entry.proof_test {
        missing.push("proof_test");
    }
    missing
}

#[instrument(level = "debug", skip(entry))]
fn gap_action(entry: &AmenableStdEntry, impl_crate: &str) -> String {
    if !entry.evidence_link {
        return format!(
            "Register `RustStdStandard<{}>` evidence in `{impl_crate}`",
            entry
                .type_path
                .rsplit("::")
                .next()
                .unwrap_or(&entry.type_path)
        );
    }
    let mut parts = Vec::new();
    if !entry.kani_witness && !entry.kani_excepted {
        parts.push("KaniWitness");
    }
    if !entry.creusot_witness && !entry.creusot_excepted {
        parts.push("Creusot witness");
    }
    if !entry.verus_witness && !entry.verus_excepted {
        parts.push("Verus witness");
    }
    if !entry.proof_test {
        parts.push("proof_chain test in proof_chain_test.rs");
    }
    format!(
        "Add {} for `{}`",
        parts.join(" + "),
        entry.evidence_name.as_deref().unwrap_or(&entry.type_path)
    )
}
