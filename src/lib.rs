//! `cordial` — polite standards for code development.
//!
//! A plugin framework for local, regeneratable lint and coverage reports.
//! See [CORDIAL_PLAN.md](https://github.com/crumplecup/cordial/blob/main/CORDIAL_PLAN.md)
//! for architecture.
//!
//! # Features
//!
//! Built-in plugins are feature-gated:
//!
//! - `panics`, `tracing`, `allows`, `modularity`, `derives`, `error_sites`, `error_chain`, `internal_error_chain`, `foreign_error_types`, `foreign_error_attenuation`, `antipatterns`, `cfg_scatter`, `visibility` — source-quality scanners
//!   (`panics` and `tracing` enabled by default)
//! - `impl_coverage`, `trenchcoat`, `shadow` — rustdoc coverage scanners
//! - `elicitation` — umbrella for all coverage plugins
//! - `full` — every built-in plugin

#[cfg(feature = "rustdoc")]
mod build;
mod cache_digest;
mod config;
#[cfg(feature = "elicitation")]
mod digest;
mod enricher;
mod error;
mod etiquette;
mod etiquettes;
mod exceptions;
mod export;
#[cfg(feature = "impl_coverage")]
mod feature_probe;
mod filter;
#[cfg(feature = "homecoming_std")]
mod framework_std;
mod hooks;
mod ir;
mod loader;
mod objects;
mod plugin;
mod plugins;
#[cfg(feature = "impl_coverage")]
mod proof_harness;
mod reporter;
#[cfg(feature = "rustdoc")]
pub mod rustdoc;
#[cfg(feature = "rustdoc")]
mod rustdoc_loader;
mod session;
mod store;
mod targets;
#[doc(hidden)]
pub mod testing;

#[cfg(feature = "allows")]
pub use etiquettes::allows::{
    ALLOWS_ETIQUETTE, AllowRuleId, scan_crate_allows, scan_rust_source as scan_allows_rust_source,
};
#[cfg(feature = "antipatterns")]
pub use etiquettes::antipatterns::{
    ANTIPATTERNS_ETIQUETTE, AntipatternRuleId, AntipatternSiteRecord, ContractRecordDump,
    scan_crate_antipatterns, scan_crate_contract_bounds, scan_creusot_contract_bounds_source,
    scan_kani_contract_bounds_source, scan_rust_source as scan_antipatterns_rust_source,
    scan_verus_contract_bounds_source,
};
#[cfg(feature = "cfg_scatter")]
pub use etiquettes::cfg_scatter::{
    CFG_SCATTER_ETIQUETTE, CfgSiteKind, scan_rust_source as scan_cfg_scatter_rust_source,
};
#[cfg(feature = "derives")]
pub use etiquettes::derives::{
    DERIVES_ETIQUETTE, DeriveRuleId, scan_rust_source as scan_derives_rust_source,
};
#[cfg(feature = "error_chain")]
pub use etiquettes::error_chain::{
    ERROR_CHAIN_ETIQUETTE, ErrorChainProbeId, ErrorChainRecord, probe_counts,
    scan_crate_error_chain, scan_rust_source as scan_error_chain_rust_source,
};
#[cfg(feature = "error_sites")]
pub use etiquettes::error_sites::{
    ERROR_SITES_ETIQUETTE, ErrorOriginClass, ErrorSiteKind, ErrorSiteScanRow,
    ForeignTypeConfidence, PartitionedErrorSiteRow, infer_foreign_error_type,
    partition_error_site_records, partition_error_site_row, scan_crate_error_sites,
    scan_rust_source as scan_error_sites_rust_source,
};
#[cfg(feature = "foreign_error_attenuation")]
pub use etiquettes::foreign_error_attenuation::{
    FOREIGN_ERROR_ATTENUATION_ETIQUETTE, ForeignErrorAttenuationReport, ForeignErrorHandlingClass,
    WorkspaceForeignErrorAttenuationSummary, build_foreign_error_attenuation_report,
    build_workspace_foreign_error_attenuation_summary,
};
#[cfg(feature = "foreign_error_types")]
pub use etiquettes::foreign_error_types::{
    ErrorSitePartitionReport, FOREIGN_ERROR_TYPES_ETIQUETTE, ForeignErrorTypeRecord,
    ForeignErrorTypeReport, WorkspaceForeignErrorTypeSummary, build_error_site_partition_report,
    build_foreign_error_type_report, build_workspace_foreign_error_type_summary,
};
#[cfg(feature = "impl_coverage")]
pub use etiquettes::impl_coverage::IMPL_COVERAGE_ETIQUETTE;
#[cfg(feature = "internal_error_chain")]
pub use etiquettes::internal_error_chain::{
    INTERNAL_ERROR_CHAIN_ETIQUETTE, InternalErrorChainScanReport, InternalErrorComplianceFinding,
    InternalErrorComplianceId, InternalErrorNodeClass, InternalErrorTypeNode,
    InternalErrorTypeProbeId, WorkspaceInternalErrorChainSummary,
    build_workspace_internal_error_chain_summary, scan_compliance_rust_source,
    scan_crate_internal_error_chain, scan_error_rust_source,
};
#[cfg(feature = "modularity")]
pub use etiquettes::modularity::{
    MODULARITY_ETIQUETTE, ModularityKind, ModuleHierarchyNode, ModuleSizeInput, ModuleSizeStats,
    OrderBand, SiblingImbalance, build_module_hierarchy, fat_leaves, library_branches,
    lopsided_siblings, order_bands, scan_rust_source as scan_modularity_rust_source,
    top_heavy_parents,
};
#[cfg(feature = "panics")]
pub use etiquettes::panics::{
    PANICS_ETIQUETTE, PanicKind, scan_crate_panics, scan_rust_source, scan_source_tree,
};
#[cfg(feature = "tracing")]
pub use etiquettes::tracing::{
    InstrumentApplySummary, InstrumentGap, TRACING_ETIQUETTE, parse_tracing_instrument_checklist,
    parse_tracing_instrument_checklist_text, run_tracing_instrument_apply,
    scan_rust_source as scan_tracing_rust_source,
};
#[cfg(feature = "trenchcoat")]
pub use etiquettes::trenchcoat::TRENCHCOAT_ETIQUETTE;
#[cfg(feature = "visibility")]
pub use etiquettes::visibility::{
    BranchingCache, VISIBILITY_ETIQUETTE, VisibilityRecord, VisibilityRuleId,
    scan_crate_visibility, scan_crate_visibility_with_cache,
};
#[cfg(feature = "shadow")]
mod shadow;
pub use cache_digest::{IrCacheDigest, SourceFileDigest};
#[cfg(feature = "elicitation")]
pub use digest::{
    ShadowCoreSupportDigest, ShadowCoreSupportStatus, ShadowCoreSupportSummary,
    TrackedTargetRosterDigest, build_shadow_core_support_digest,
    render_shadow_core_support_summary_section, render_tracked_target_roster_markdown,
};
#[cfg(any(
    feature = "panics",
    feature = "tracing",
    feature = "allows",
    feature = "modularity",
    feature = "derives",
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "foreign_error_types",
    feature = "foreign_error_attenuation",
    feature = "antipatterns",
    feature = "cfg_scatter",
    feature = "visibility"
))]
pub use enricher::AttributeEnricher;
pub use enricher::ScopeEnricher;
#[cfg(feature = "trenchcoat")]
pub use enricher::TrenchcoatEnricher;
#[cfg(feature = "error_sites")]
pub use enricher::{
    ERROR_IR_ENRICHERS, ErrorFlowEnricher, ErrorIrScanEnricher, ErrorIrScanReport,
    error_ir_enricher_ids, scan_crate_error_ir,
};
#[cfg(feature = "impl_coverage")]
pub use enricher::{
    FeatureProbeEnricher, ProofHarnessEnricher, TraitImplEnricher, WrapperCoverageEnricher,
};
pub use enricher::{PathIndexEnricher, SynDocLinkEnricher, syn_doc_peer};
#[cfg(feature = "shadow")]
pub use enricher::{
    ShadowLinkEnricher, ShadowMapEntry, discover_same_crate_shadow_pairs, load_shadow_map,
    resolve_shadow_entries,
};
pub use error::{CordialError, CordialResult, TokenStreamParseError};
pub use etiquette::{Etiquette, StaticEtiquette};
#[cfg(feature = "elicitation")]
pub use etiquettes::coverage_etiquettes;
#[cfg(feature = "amenable_std")]
pub use etiquettes::framework_std::{AMENABLE_STD_ETIQUETTE, AmenableStdReporter};
pub use etiquettes::quality_etiquettes;
#[cfg(feature = "shadow")]
pub use etiquettes::shadow::SHADOW_ETIQUETTE;
#[cfg(all(feature = "rustdoc", feature = "impl_coverage"))]
pub use plugin::classify_elicit_complete_gap;
#[cfg(feature = "elicitation")]
pub use plugin::{
    ElicitationTargetProvider, ShadowPair, TrackedTargetRosterGap, active_tracked_targets,
    compare_tracked_target_roster, discover_active_shadow_pairs, is_interface_shadow_crate,
    tracked_target_for_shadow, tracked_target_for_upstream,
};
pub use plugin::{
    ErrorHandling, ErrorHandlingLayers, ErrorHandlingPolicy, ErrorScope, ErrorScopeProvider,
    ErrorSurface, StandardErrorHandlingPolicy, WorkspaceMembersErrorScopeProvider,
};
pub use plugin::{
    EtiquettePlugin, Plugin, PluginCategory, StaticPlugin, etiquettes_from_plugins,
    plugins_in_category, selected_plugins,
};
#[cfg(feature = "amenable_std")]
pub use plugins::{AMENABLE_STD_COVERAGE, AmenableStdCoverage};
#[cfg(feature = "elicitation")]
pub use plugins::{ELICITATION_COVERAGE, ElicitationCoverage};
#[cfg(any(feature = "error_sites", feature = "panics"))]
pub use plugins::{
    STANDARD_ERROR_HANDLING, StandardErrorHandling, error_handling_only_plugins,
    error_handling_plugins, standard_error_handling_etiquettes,
};
pub use plugins::{
    all_etiquettes_from_plugins, all_plugins, coverage_only_plugins, coverage_plugins,
    quality_only_plugins, quality_plugins,
};
// `homecoming_std` re-exports live in one block (see docs/planning/cfg-scatter-etiquette.md)
// even though they source from several internal modules.
#[cfg(all(feature = "rustdoc", feature = "shadow"))]
pub use build::{
    build_active_shadow_deps, build_all_active_shadow_deps, build_shadow_dep_rustdoc,
    resolve_shadow_dep_build_config,
};
pub use config::{
    CfgScatterThresholds, CordialConfig, ModularityThresholds, TracingThresholds,
    VisibilityThresholds, load_cordial_config, load_session_config, load_visibility_thresholds,
};
pub use exceptions::{ExceptionEntry, ExceptionSet, apply_exception_sets, load_exceptions};
pub use export::{SurrealEdge, SurrealGraphExport, SurrealNode, surreal_statements};
pub use filter::NamedRunFilter;
pub use hooks::{Assessor, IrEnricher, Loader, Probe, Reporter, WorkspaceAssessor};
pub use ir::{
    ATTR_IR_ORIGIN, ATTR_SYN_DOC_PEER, AttrKey, AttrValue, BasicQuery, CrateIr, CrateIrSnapshot,
    CrateView, CrateViewMut, EdgeKind, EdgeWeight, IrIndexes, IrMut, IrView, ItemKind, NodeId,
    NodeKind, NodeView, NodeWeight, ORIGIN_RUSTDOC, ORIGIN_SOURCE, PanicSitesQuery, QualifiedPath,
    Query, QueryBuilder, WorkspaceIr,
};
#[cfg(feature = "impl_coverage")]
pub use ir::{
    build_wrapper_coverage_from_hub_ir, collect_trenchcoat_pairs_from_ir, wrapper_maps_equivalent,
};
pub use loader::{
    CrateTarget, LoadView, SourceFile, SourceLoadView, SourceLoader, module_path_from_src_file,
    quality_scan_trees,
};
pub use objects::{
    Artifact, Disposition, FileSpan, Finding, FindingSink, IrAnchor, MapFindingSink, Marker,
    NodeAnchor, Rule, SourceSpan, TextArtifact,
};
pub use reporter::RollupReporter;
#[cfg(any(
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]
pub use reporter::{
    CoveragePluginSummary, CoverageSummary, build_coverage_summary,
    render_coverage_summary_markdown,
};
#[cfg(feature = "quality")]
pub use reporter::{
    QualityAreaSummary, QualityReport, QualityReportReporter, build_quality_report,
    render_quality_report_markdown, render_quality_workspace_summary_markdown,
};
pub use session::{
    RunAll, RunFilter, RunOutcome, RuntimeSession, Session, SessionBuilder, SessionView,
};
pub use store::{StoreLayout, SysrootCache, default_store_home, project_slug_from_path};
#[cfg(feature = "homecoming_std")]
pub use {
    build::build_sysroot_libraries,
    etiquettes::framework_std::HOMECOMING_STD_ETIQUETTE,
    plugin::{WorkspaceHub, detect_workspace_hub, discover_workspace_hub},
    plugins::{HOMECOMING_STD_COVERAGE, HomecomingStdCoverage, coverage_plugins_for_hub},
};
// `rustdoc` re-exports live in one block (see docs/planning/cfg-scatter-etiquette.md)
// even though they source from several internal modules.
pub use targets::{discover_crate_targets, discover_run_crate_targets};
#[cfg(feature = "rustdoc")]
pub use {
    build::{
        BuildArtifact, BuildKind, BuildOptions, DocFingerprint, build_workspace_members,
        nightly_available, run_cargo_rustdoc,
    },
    enricher::RustdocStructureEnricher,
    ir::{
        count_type_nodes, mirror_target, rustdoc_item_nodes, type_elicit_complete,
        type_public_methods, type_trait_impls, type_trait_prereqs, type_wraps_foreign,
    },
    plugin::{
        Coverage, CoverageTarget, CoverageTargetKind, ElicitCompleteRequirement, GapContext,
        TargetProvider, TraitRequirement, WorkspaceMembersTargetProvider,
    },
    rustdoc::{
        ELICIT_COMPLETE_SUPERTRAITS, ELICIT_COMPLETE_TRAIT, ElicitCompleteSet, TraitPrereqs,
        WrapperCoverage, WrapperCoverageMap, build_wrapper_coverage_map,
        collect_elicit_complete_paths, collect_trait_prereqs_for_inventory,
        lookup_wrapper_coverage, prereqs_from_trait_shorts,
    },
    rustdoc_loader::{RustdocLoadView, RustdocLoader, resolve_rustdoc_json},
};
