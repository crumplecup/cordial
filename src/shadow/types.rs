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
#[derive(Debug, Clone, derive_getters::Getters, derive_builder::Builder, PartialEq, Eq)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ShadowRow {
    /// Qualified path of the inventory item.
    item_path: String,
    /// rustdoc inventory kind of this item.
    #[getter(copy)]
    item_kind: InventoryItemKind,
    /// Rollup status for this row.
    #[getter(copy)]
    status: ShadowStatus,
    /// Matching shadow path, when one exists.
    shadow_item: String,
    /// How confident the compare is that this is drift vs a rename.
    drift_confidence: String,
    /// Whether the shadow item impls the elicitation trait.
    shadow_elicit_impl: String,
    /// Whether the shadow item can take a direct elicitation impl.
    shadow_can_be_direct: String,
    /// External traits still missing on the shadow item.
    shadow_missing_external_traits: String,
    /// Our traits still missing on the shadow item.
    shadow_missing_our_traits: String,
    /// Free-form notes for the report row.
    notes: String,
}

impl ShadowRow {
    /// Start a builder for this row.
    #[instrument(level = "debug")]
    pub fn builder() -> ShadowRowBuilder {
        ShadowRowBuilder::default()
    }
}

/// Full shadow-mirror report for one target crate.
#[derive(Debug, Clone, derive_getters::Getters, derive_builder::Builder, PartialEq)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ShadowReport {
    /// Upstream crate being compared or covered.
    target_crate: String,
    /// Shadow crate that should mirror the target.
    shadow_crate: String,
    /// Per-item compare rows.
    rows: Vec<ShadowRow>,
    /// How many items are covered.
    #[getter(copy)]
    covered_count: usize,
    /// How many items are still missing.
    #[getter(copy)]
    missing_count: usize,
    /// How many items drifted.
    #[getter(copy)]
    drifted_count: usize,
    /// How many extra shadow-only items were found.
    #[getter(copy)]
    extra_count: usize,
    /// Covered fraction as a percentage.
    #[getter(copy)]
    coverage_pct: f64,
    /// How many items have a verification gap.
    #[getter(copy)]
    verification_gap_count: usize,
    /// Per-type method coverage for matched pairs.
    method_coverage: Vec<TypeMethodCoverage>,
    /// Matched types whose methods are still missing.
    missing_type_methods: Vec<TypeMethodCoverage>,
    /// Per-trait impl coverage for the shadow crate.
    trait_coverage: Vec<TraitImplCoverage>,
}

impl ShadowReport {
    /// Start a builder for this report.
    #[instrument(level = "debug")]
    pub fn builder() -> ShadowReportBuilder {
        ShadowReportBuilder::default()
    }
}

/// Method-level coverage for one matched upstream ↔ shadow type pair.
#[derive(Debug, Clone, derive_getters::Getters, derive_new::new, PartialEq, Eq)]
pub struct TypeMethodCoverage {
    /// Upstream type path.
    upstream_type: String,
    /// Matching shadow type path.
    shadow_type: String,
    /// Names present on both sides.
    covered: Vec<String>,
    /// Names present upstream but missing on the shadow.
    missing: Vec<String>,
    /// Names present on the shadow with no upstream match.
    extra: Vec<String>,
}

impl TypeMethodCoverage {
    /// Upstream method count.
    #[instrument(level = "trace", skip(self))]
    pub fn upstream_method_count(&self) -> usize {
        self.covered.len() + self.missing.len()
    }
}

/// Trait-impl coverage for one upstream trait missing from the shadow inventory.
#[derive(Debug, Clone, derive_getters::Getters, derive_new::new, PartialEq, Eq)]
pub struct TraitImplCoverage {
    /// Qualified path of the trait.
    trait_path: String,
    /// Upstream impls missing from the shadow type.
    missing_on_shadow: Vec<String>,
    /// Upstream impls also present on the shadow type.
    covered_on_shadow: Vec<String>,
}

/// Optional method/trait maps passed into [`super::report::build_shadow_report`].
#[derive(Debug, derive_getters::Getters, derive_new::new)]
pub struct ShadowBuildMaps<'a> {
    /// Upstream type → method names.
    #[getter(copy)]
    target_methods: &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
    /// Shadow type → method names.
    #[getter(copy)]
    shadow_methods: &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
    /// Upstream type → trait impls.
    #[getter(copy)]
    target_trait_impls: &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
    /// Shadow type → trait impls.
    #[getter(copy)]
    shadow_trait_impls: &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
}

impl ShadowBuildMaps<'static> {
    /// Empty.
    #[instrument(level = "debug")]
    pub fn empty() -> Self {
        static EMPTY: std::sync::OnceLock<
            std::collections::HashMap<String, std::collections::BTreeSet<String>>,
        > = std::sync::OnceLock::new();
        let empty = EMPTY.get_or_init(std::collections::HashMap::new);
        Self::new(empty, empty, empty, empty)
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
#[derive(Debug, Clone, derive_getters::Getters, derive_builder::Builder, PartialEq, Eq)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ShadowGapEntry {
    /// Upstream crate being compared or covered.
    target_crate: String,
    /// Shadow crate that should mirror the target.
    shadow_crate: String,
    /// Qualified path of the inventory item.
    item_path: String,
    /// rustdoc inventory kind of this item.
    item_kind: String,
    /// How this coverage gap is classified.
    #[getter(copy)]
    gap_kind: ShadowGapKind,
    /// Shadow path matched to this gap, if any.
    matched_shadow_item: String,
    /// How confident the compare is that this is drift vs a rename.
    drift_confidence: String,
    /// Whether the shadow item impls the elicitation trait.
    shadow_elicit_impl: String,
    /// Whether the shadow item can take a direct elicitation impl.
    shadow_can_be_direct: String,
    /// External traits still missing on the shadow item.
    shadow_missing_external_traits: String,
    /// Our traits still missing on the shadow item.
    shadow_missing_our_traits: String,
    /// Recommended next action for this gap.
    action: String,
    /// Free-form notes for the report row.
    notes: String,
}

impl ShadowGapEntry {
    /// Start a builder for this gap.
    #[instrument(level = "debug")]
    pub fn builder() -> ShadowGapEntryBuilder {
        ShadowGapEntryBuilder::default()
    }
}
