//! Cross-crate shadow mirror compare (upstream vs `elicit_*` inventories).

mod build;
mod checklist;
mod gaps;
mod ir;
mod matching;
mod pair;
mod types;
mod verification;

pub use build::{
    build_shadow_report, build_shadow_report_from_inventories,
    build_shadow_report_from_inventories_with_maps,
};
pub use checklist::render_shadow_method_checklist;
pub use gaps::{api_family, build_shadow_gaps, render_shadow_row};
pub use ir::build_shadow_pair_report_from_workspace_ir;
pub use pair::{
    build_shadow_pair_report_from_workspace, load_workspace_shadow_reports,
    preload_shadow_pair_crates,
};
pub use types::{
    ShadowBuildMaps, ShadowGapEntry, ShadowGapKind, ShadowReport, ShadowRow, ShadowStatus,
    TraitImplCoverage, TypeMethodCoverage,
};
