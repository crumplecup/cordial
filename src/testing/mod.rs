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
    crate::etiquettes::{ImplGapAssessment, ImplGapKind, assess_impl_gap},
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
    type_has_trait_impl,
};

#[cfg(any(feature = "amenable_std", feature = "antipatterns"))]
pub use crate::amenable_dump_registry::{AMENABLE_DUMP_REGISTRY_FEATURES, registry_dump_is_fresh};

#[cfg(feature = "amenable_std")]
pub use crate::framework_std::{
    AmenableStdOptions, AmenableStdReport, AmenableStdStatus, CrateIndex, EvidenceKey,
    EvidenceKind, EvidenceLinkDump, Lookup, PremiseDump, ProofRecordDump, RegistryDump,
    ResolveCaps, RustdocTypeResolver, TypeKey, TypeResolver, TypeText, Unresolved,
    VerifierSkipEntry, VerifierSkipMap, assess_amenable_std_coverage, build_amenable_std_gaps,
    build_amenable_std_report, evidence_for_std_type, load_verifier_skip_map, normalize_type_text,
    parse_rust_std_standard_inner, parse_type_text, resolve_alias_chain, resolve_ext_evidence,
    witness_verifiers_for_std_type,
};

#[cfg(feature = "amenable_ext")]
pub use crate::framework_std::{
    AMENABLE_EXT_IMPL_CRATE, AMENABLE_EXT_JIFF_PATCH_SET, AMENABLE_EXT_JIFF_UPSTREAM_CRATE,
    AmenableExtOptions, assess_amenable_ext_coverage, build_amenable_ext_gaps,
    build_amenable_ext_report, collect_proof_chain_subjects, evidence_for_ext_type,
    generic_claims_for_ext_type, load_ext_inventory_from_shadow_dep, parse_ext_generic_inner,
    parse_ext_standard_inner, render_amenable_ext_checklist_md, render_amenable_ext_summary_md,
    witness_verifiers_for_ext_type,
};

#[cfg(feature = "rustdoc")]
pub use crate::enricher::inventory_link_key;

#[cfg(feature = "rustdoc")]
pub use crate::cargo_rustdoc::rustdoc_cache_is_fresh;

#[cfg(feature = "elicitation")]
pub use crate::digest::{ImplCrateRollup, build_shadow_core_support_summary};

#[cfg(feature = "rustdoc")]
pub use crate::rustdoc::{
    InventoryItemKind, RustdocInventory, RustdocItem, StabilityLevel, collect_trait_impls,
    collect_trenchcoat_pairs, parse_rustdoc_json, parse_stability_attr_text,
    rustdoc_json_has_stability_markers, stability_from_attrs,
};
