//! Hidden exports for in-repo parity tests and oracles.

#[cfg(feature = "shadow")]
mod shadow_oracle;

#[cfg(feature = "impl_coverage")]
mod wrapper_oracle;

#[cfg(feature = "rustdoc")]
mod load_view;

#[cfg(feature = "shadow")]
pub use shadow_oracle::{build_shadow_pair_report, build_shadow_pair_report_from_inventories};

#[cfg(feature = "shadow")]
pub use crate::shadow::{
    ShadowBuildMaps, ShadowGapEntry, ShadowGapKind, ShadowReport, ShadowRow, ShadowStatus,
    TraitImplCoverage, TypeMethodCoverage, build_shadow_gaps,
    build_shadow_pair_report_from_workspace_ir, build_shadow_report,
    build_shadow_report_from_inventories, build_shadow_report_from_inventories_with_maps,
    load_workspace_shadow_reports, render_shadow_method_checklist,
};

#[cfg(feature = "rustdoc")]
pub use load_view::rustdoc_load_view;

#[cfg(feature = "impl_coverage")]
pub use {
    self::wrapper_oracle::load_workspace_wrapper_coverage,
    crate::cargo_rustdoc::{
        DepBuildConfig, collect_dep_serde_features, collect_member_dep_build_config,
    },
    crate::etiquettes::impl_coverage::{ImplGapAssessment, ImplGapKind, assess_impl_gap},
    crate::feature_probe::{
        TypeFeatureProbe, build_type_feature_probes, hub_crate_name, load_crate_feature_probes,
    },
    crate::ir::{
        build_wrapper_coverage_from_hub_ir, collect_trenchcoat_pairs_from_ir,
        wrapper_maps_equivalent,
    },
    crate::proof_harness::{
        ProofHarness, TestStatus, collect_proof_harness, load_workspace_proof_harness,
        test_status_for_type_path,
    },
    crate::rustdoc::ensure_workspace_wrapper_coverage,
};

#[cfg(feature = "homecoming_std")]
pub use crate::framework_std::{
    FrameworkGapEntry, FrameworkStdOptions, FrameworkTraitEntry, FrameworkTraitReport,
    FrameworkTraitStatus, HOMECOMING_IMPL_CRATE, HOMECOMING_TRAIT, SkipMap, StdInventoryItem,
    assess_homecoming_std_coverage, build_framework_gaps, build_framework_trait_report,
    framework_std_type_items, load_merged_std_inventory, merge_std_inventory_items,
};

#[cfg(feature = "amenable_std")]
pub use crate::framework_std::{
    AmenableStdOptions, AmenableStdReport, AmenableStdStatus, EvidenceLinkDump, ProofRecordDump,
    RegistryDump, VerifierSkipEntry, VerifierSkipMap, assess_amenable_std_coverage,
    build_amenable_std_gaps, build_amenable_std_report, evidence_for_std_type,
    load_verifier_skip_map, parse_rust_std_standard_inner, witness_verifiers_for_std_type,
};

#[cfg(feature = "rustdoc")]
pub use crate::rustdoc::{
    InventoryItemKind, RustdocInventory, RustdocItem, StabilityLevel, collect_trait_impls,
    collect_trenchcoat_pairs, parse_rustdoc_json, parse_stability_attr_text,
    rustdoc_json_has_stability_markers, stability_from_attrs,
};
