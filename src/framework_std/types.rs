//! Framework std coverage row types.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::rustdoc::InventoryItemKind;

/// Whether a std type has the tracked framework trait impl.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameworkTraitStatus {
    Complete,
    Missing,
    Skipped,
}

impl std::fmt::Display for FrameworkTraitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Complete => write!(f, "Complete"),
            Self::Missing => write!(f, "Missing"),
            Self::Skipped => write!(f, "Skipped"),
        }
    }
}

/// One std inventory row assessed for framework trait coverage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdInventoryItem {
    pub path: String,
    pub kind: InventoryItemKind,
    pub is_generic: bool,
    pub is_unstable: bool,
    /// For type aliases, the aliased type path (used by amenable registry matching).
    pub alias_target: Option<String>,
}

/// One row in a framework trait coverage report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkTraitEntry {
    pub type_path: String,
    pub type_kind: String,
    pub is_generic: bool,
    pub trait_status: FrameworkTraitStatus,
    pub skip_reason: Option<String>,
}

/// Coverage report for merged std-family inventory vs impl-crate trait impls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkTraitReport {
    pub source_crate: String,
    pub trait_name: String,
    pub impl_crate: String,
    pub include_nightly: bool,
    pub entries: Vec<FrameworkTraitEntry>,
    pub complete_count: usize,
    pub missing_count: usize,
    pub skipped_count: usize,
}

impl FrameworkTraitReport {
    pub fn coverage_pct(&self) -> f32 {
        let accountable = self.entries.len().saturating_sub(self.skipped_count);
        if accountable == 0 {
            0.0
        } else {
            self.complete_count as f32 / accountable as f32 * 100.0
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "{} {} trait {} ({} complete, {} missing, {} skipped, {:.1}% of accountable)",
            self.entries.len(),
            self.trait_name,
            self.source_crate,
            self.complete_count,
            self.missing_count,
            self.skipped_count,
            self.coverage_pct()
        )
    }
}

/// One actionable gap row for framework trait coverage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkGapEntry {
    pub source_crate: String,
    pub type_path: String,
    pub type_kind: String,
    pub trait_name: String,
    pub impl_crate: String,
    pub action: String,
}

pub type SkipMap = std::collections::HashMap<String, String>;

/// Classify one std inventory row for framework trait coverage.
pub fn classify_framework_std_row(
    type_path: &str,
    impl_paths: &HashSet<String>,
    skip_map: &SkipMap,
) -> (FrameworkTraitStatus, Option<String>) {
    if let Some(reason) = skip_map.get(type_path) {
        return (FrameworkTraitStatus::Skipped, Some(reason.clone()));
    }
    if crate::framework_std::match_impl::type_has_trait_impl(impl_paths, type_path) {
        (FrameworkTraitStatus::Complete, None)
    } else {
        (FrameworkTraitStatus::Missing, None)
    }
}

/// Build a framework trait coverage report from std inventory and impl-crate trait paths.
pub fn build_framework_trait_report(
    source_crate: &str,
    items: &[StdInventoryItem],
    trait_name: &str,
    impl_crate: &str,
    impl_paths: &HashSet<String>,
    skip_map: &SkipMap,
    include_nightly: bool,
) -> FrameworkTraitReport {
    let mut entries = Vec::new();
    let mut complete_count = 0usize;
    let mut missing_count = 0usize;
    let mut skipped_count = 0usize;

    for item in framework_std_type_items(items, include_nightly) {
        let type_path = item.path.clone();
        let (trait_status, skip_reason) =
            classify_framework_std_row(&type_path, impl_paths, skip_map);
        match trait_status {
            FrameworkTraitStatus::Complete => complete_count += 1,
            FrameworkTraitStatus::Missing => missing_count += 1,
            FrameworkTraitStatus::Skipped => skipped_count += 1,
        };
        entries.push(FrameworkTraitEntry {
            type_path,
            type_kind: item.kind.as_str().to_string(),
            is_generic: item.is_generic,
            trait_status,
            skip_reason,
        });
    }

    FrameworkTraitReport {
        source_crate: source_crate.to_string(),
        trait_name: trait_name.to_string(),
        impl_crate: impl_crate.to_string(),
        include_nightly,
        entries,
        complete_count,
        missing_count,
        skipped_count,
    }
}

/// Build consolidated gap rows from a framework trait report.
pub fn build_framework_gaps(report: &FrameworkTraitReport) -> Vec<FrameworkGapEntry> {
    report
        .entries
        .iter()
        .filter(|entry| entry.trait_status == FrameworkTraitStatus::Missing)
        .map(|entry| FrameworkGapEntry {
            source_crate: report.source_crate.clone(),
            type_path: entry.type_path.clone(),
            type_kind: entry.type_kind.clone(),
            trait_name: report.trait_name.clone(),
            impl_crate: report.impl_crate.clone(),
            action: format!(
                "Add `impl {} for {}` in {}",
                report.trait_name, entry.type_path, report.impl_crate
            ),
        })
        .collect()
}

/// Std-family inventory rows that count toward framework trait coverage.
pub fn framework_std_type_items(
    items: &[StdInventoryItem],
    include_nightly: bool,
) -> impl Iterator<Item = &StdInventoryItem> {
    items.iter().filter(move |item| {
        if !include_nightly && item.is_unstable {
            return false;
        }
        item.kind.is_type() || is_rustdoc_primitive(item)
    })
}

fn is_rustdoc_primitive(item: &StdInventoryItem) -> bool {
    item.kind == InventoryItemKind::Other
        && item.path.split("::").count() == 2
        && matches!(item.path.split("::").next(), Some("std" | "core" | "alloc"))
}

/// Merge concrete type items from multiple std-family inventories, deduped by path.
pub fn merge_std_inventory_items(inventories: &[Vec<StdInventoryItem>]) -> Vec<StdInventoryItem> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    for inventory in inventories {
        for item in inventory {
            if seen.insert(item.path.clone()) {
                items.push(item.clone());
            }
        }
    }
    items.sort_by(|left, right| left.path.cmp(&right.path));
    items
}
