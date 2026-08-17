//! Framework std trait coverage — homecoming `Code` and amenable registry profiles.

mod inventory;
mod match_impl;
mod render;
mod run;
mod skip;
mod types;

#[cfg(feature = "amenable_std")]
mod amenable;
#[cfg(feature = "amenable_std")]
mod amenable_render;
#[cfg(feature = "amenable_std")]
mod amenable_run;
#[cfg(feature = "amenable_std")]
mod proof_harness;
#[cfg(feature = "amenable_std")]
mod registry;
#[cfg(feature = "amenable_std")]
mod verifier_skip;

pub use inventory::{FRAMEWORK_STD_SOURCES, load_merged_std_inventory};
pub use render::{
    render_framework_checklist_md, render_framework_coverage_csv, render_framework_gaps_csv,
    render_framework_summary_md,
};
pub use run::{
    FrameworkStdOptions, HOMECOMING_IMPL_CRATE, HOMECOMING_PATCH_SET, HOMECOMING_TRAIT,
    assess_homecoming_std_coverage,
};
pub use skip::load_framework_skip_map;
pub use types::{
    FrameworkGapEntry, FrameworkTraitEntry, FrameworkTraitReport, FrameworkTraitStatus, SkipMap,
    StdInventoryItem, build_framework_gaps, build_framework_trait_report,
    classify_framework_std_row, framework_std_type_items, merge_std_inventory_items,
};

#[cfg(feature = "amenable_std")]
pub use self::{
    amenable::{
        AmenableStdEntry, AmenableStdGapEntry, AmenableStdReport, AmenableStdStatus,
        amenable_gap_fields, build_amenable_std_gaps, build_amenable_std_report,
        classify_amenable_std_row,
    },
    amenable_render::{
        render_amenable_std_checklist_md, render_amenable_std_coverage_csv,
        render_amenable_std_gaps_csv, render_amenable_std_summary_md,
    },
    amenable_run::{
        AMENABLE_IMPL_CRATE, AMENABLE_PATCH_SET, AmenableStdOptions, assess_amenable_std_coverage,
        ensure_registry_dump_for_assessor,
    },
    proof_harness::collect_proof_chain_subjects,
    registry::{
        EvidenceLinkDump, ProofRecordDump, RegistryDump, evidence_for_std_type,
        parse_rust_std_standard_inner, witness_verifiers_for_std_type,
    },
    verifier_skip::{VerifierSkipEntry, VerifierSkipMap, load_verifier_skip_map},
};
