//! Shadow mirror compare types.

use crate::rustdoc::InventoryItemKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowStatus {
    Covered,
    Missing,
    Drifted,
    Extra,
}

impl ShadowStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Covered => "Covered",
            Self::Missing => "Missing",
            Self::Drifted => "Drifted",
            Self::Extra => "Extra",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowRow {
    pub item_path: String,
    pub item_kind: InventoryItemKind,
    pub status: ShadowStatus,
    pub shadow_item: String,
    pub drift_confidence: String,
    pub shadow_elicit_impl: String,
    pub shadow_can_be_direct: String,
    pub shadow_missing_external_traits: String,
    pub shadow_missing_our_traits: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShadowReport {
    pub target_crate: String,
    pub shadow_crate: String,
    pub rows: Vec<ShadowRow>,
    pub covered_count: usize,
    pub missing_count: usize,
    pub drifted_count: usize,
    pub extra_count: usize,
    pub coverage_pct: f64,
    pub verification_gap_count: usize,
    pub method_coverage: Vec<TypeMethodCoverage>,
    pub missing_type_methods: Vec<TypeMethodCoverage>,
    pub trait_coverage: Vec<TraitImplCoverage>,
}

/// Method-level coverage for one matched upstream ↔ shadow type pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMethodCoverage {
    pub upstream_type: String,
    pub shadow_type: String,
    pub covered: Vec<String>,
    pub missing: Vec<String>,
    pub extra: Vec<String>,
}

impl TypeMethodCoverage {
    pub fn upstream_method_count(&self) -> usize {
        self.covered.len() + self.missing.len()
    }
}

/// Trait-impl coverage for one upstream trait missing from the shadow inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitImplCoverage {
    pub trait_path: String,
    pub missing_on_shadow: Vec<String>,
    pub covered_on_shadow: Vec<String>,
}

/// Optional method/trait maps passed into [`super::build::build_shadow_report`].
pub struct ShadowBuildMaps<'a> {
    pub target_methods: &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
    pub shadow_methods: &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
    pub target_trait_impls:
        &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
    pub shadow_trait_impls:
        &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
}

impl ShadowBuildMaps<'static> {
    pub fn empty() -> Self {
        static EMPTY: std::sync::OnceLock<
            std::collections::HashMap<String, std::collections::BTreeSet<String>>,
        > = std::sync::OnceLock::new();
        let empty = EMPTY.get_or_init(std::collections::HashMap::new);
        Self {
            target_methods: empty,
            shadow_methods: empty,
            target_trait_impls: empty,
            shadow_trait_impls: empty,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowGapKind {
    Missing,
    Drifted,
    PossiblyStale,
    InfrastructureExtra,
    ShadowVerificationGap,
}

impl ShadowGapKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "Missing",
            Self::Drifted => "Drifted",
            Self::PossiblyStale => "PossiblyStale",
            Self::InfrastructureExtra => "InfrastructureExtra",
            Self::ShadowVerificationGap => "ShadowVerificationGap",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowGapEntry {
    pub target_crate: String,
    pub shadow_crate: String,
    pub item_path: String,
    pub item_kind: String,
    pub gap_kind: ShadowGapKind,
    pub matched_shadow_item: String,
    pub drift_confidence: String,
    pub shadow_elicit_impl: String,
    pub shadow_can_be_direct: String,
    pub shadow_missing_external_traits: String,
    pub shadow_missing_our_traits: String,
    pub action: String,
    pub notes: String,
}
