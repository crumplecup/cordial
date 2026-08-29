//! Shadow mirror compare types.

use crate::rustdoc::InventoryItemKind;

use tracing::instrument;
/// Coverage status of one shadow-compare row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowStatus {
    /// Covered.
    Covered,
    /// Missing.
    Missing,
    /// Drifted.
    Drifted,
    /// Extra.
    Extra,
}

impl ShadowStatus {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Covered => "Covered",
            Self::Missing => "Missing",
            Self::Drifted => "Drifted",
            Self::Extra => "Extra",
        }
    }
}

/// One upstream ↔ shadow compare row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowRow {
    /// Qualified path of the inventory item.
    pub item_path: String,
    /// rustdoc inventory kind of this item.
    pub item_kind: InventoryItemKind,
    /// Rollup status for this row.
    pub status: ShadowStatus,
    /// Matching shadow path, when one exists.
    pub shadow_item: String,
    /// How confident the compare is that this is drift vs a rename.
    pub drift_confidence: String,
    /// Whether the shadow item impls the elicitation trait.
    pub shadow_elicit_impl: String,
    /// Whether the shadow item can take a direct elicitation impl.
    pub shadow_can_be_direct: String,
    /// External traits still missing on the shadow item.
    pub shadow_missing_external_traits: String,
    /// Our traits still missing on the shadow item.
    pub shadow_missing_our_traits: String,
    /// Free-form notes for the report row.
    pub notes: String,
}

/// Full shadow-mirror report for one target crate.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowReport {
    /// Upstream crate being compared or covered.
    pub target_crate: String,
    /// Shadow crate that should mirror the target.
    pub shadow_crate: String,
    /// Per-item compare rows.
    pub rows: Vec<ShadowRow>,
    /// How many items are covered.
    pub covered_count: usize,
    /// How many items are still missing.
    pub missing_count: usize,
    /// How many items drifted.
    pub drifted_count: usize,
    /// How many extra shadow-only items were found.
    pub extra_count: usize,
    /// Covered fraction as a percentage.
    pub coverage_pct: f64,
    /// How many items have a verification gap.
    pub verification_gap_count: usize,
    /// Per-type method coverage for matched pairs.
    pub method_coverage: Vec<TypeMethodCoverage>,
    /// Matched types whose methods are still missing.
    pub missing_type_methods: Vec<TypeMethodCoverage>,
    /// Per-trait impl coverage for the shadow crate.
    pub trait_coverage: Vec<TraitImplCoverage>,
}

/// Method-level coverage for one matched upstream ↔ shadow type pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMethodCoverage {
    /// Upstream type path.
    pub upstream_type: String,
    /// Matching shadow type path.
    pub shadow_type: String,
    /// Names present on both sides.
    pub covered: Vec<String>,
    /// Names present upstream but missing on the shadow.
    pub missing: Vec<String>,
    /// Names present on the shadow with no upstream match.
    pub extra: Vec<String>,
}

impl TypeMethodCoverage {
    /// Upstream method count.
    #[instrument(level = "trace", skip(self))]
    pub fn upstream_method_count(&self) -> usize {
        self.covered.len() + self.missing.len()
    }
}

/// Trait-impl coverage for one upstream trait missing from the shadow inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitImplCoverage {
    /// Qualified path of the trait.
    pub trait_path: String,
    /// Upstream impls missing from the shadow type.
    pub missing_on_shadow: Vec<String>,
    /// Upstream impls also present on the shadow type.
    pub covered_on_shadow: Vec<String>,
}

/// Optional method/trait maps passed into [`super::report::build_shadow_report`].
#[derive(Debug)]
pub struct ShadowBuildMaps<'a> {
    /// Upstream type → method names.
    pub target_methods: &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
    /// Shadow type → method names.
    pub shadow_methods: &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
    /// Upstream type → trait impls.
    pub target_trait_impls:
        &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
    /// Shadow type → trait impls.
    pub shadow_trait_impls:
        &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
}

impl ShadowBuildMaps<'static> {
    /// Empty.
    #[instrument(level = "debug")]
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

/// Classification of a shadow coverage gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowGapKind {
    /// Missing.
    Missing,
    /// Drifted.
    Drifted,
    /// PossiblyStale.
    PossiblyStale,
    /// InfrastructureExtra.
    InfrastructureExtra,
    /// ShadowVerificationGap.
    ShadowVerificationGap,
}

impl ShadowGapKind {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
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

/// One classified gap in a shadow report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowGapEntry {
    /// Upstream crate being compared or covered.
    pub target_crate: String,
    /// Shadow crate that should mirror the target.
    pub shadow_crate: String,
    /// Qualified path of the inventory item.
    pub item_path: String,
    /// rustdoc inventory kind of this item.
    pub item_kind: String,
    /// How this coverage gap is classified.
    pub gap_kind: ShadowGapKind,
    /// Shadow path matched to this gap, if any.
    pub matched_shadow_item: String,
    /// How confident the compare is that this is drift vs a rename.
    pub drift_confidence: String,
    /// Whether the shadow item impls the elicitation trait.
    pub shadow_elicit_impl: String,
    /// Whether the shadow item can take a direct elicitation impl.
    pub shadow_can_be_direct: String,
    /// External traits still missing on the shadow item.
    pub shadow_missing_external_traits: String,
    /// Our traits still missing on the shadow item.
    pub shadow_missing_our_traits: String,
    /// Recommended next action for this gap.
    pub action: String,
    /// Free-form notes for the report row.
    pub notes: String,
}
