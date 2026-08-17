//! Elicitation-specific cordial etiquettes.
//!
//! This crate is the delegation target for [`elicit_doc`] coverage analysis.
//! It owns the tracked-target roster and re-exports the [`ElicitationCoverage`] plugin.
//!
//! [`elicit_doc`]: https://github.com/crumplecup/elicit_doc

mod coverage;
mod tracked_targets;

pub use coverage::{ELICITATION_COVERAGE, ElicitationCoverage};
pub use tracked_targets::{
    ELICITATION_INTERFACE_SHADOW_CRATES, ELICITATION_TRACKED_TARGETS, ElicitationTrackedTarget,
};

/// Target-provider helpers (implemented in `cordial`; re-exported for profile users).
pub use cordial::{
    ElicitationTargetProvider, ShadowPair, TrackedTargetRosterGap, active_tracked_targets,
    compare_tracked_target_roster, discover_active_shadow_pairs, is_interface_shadow_crate,
    tracked_target_for_shadow, tracked_target_for_upstream,
};

/// Shadow core support digest (implemented in `cordial`; re-exported for summary tooling).
pub use cordial::{
    ShadowCoreSupportDigest, ShadowCoreSupportStatus, ShadowCoreSupportSummary,
    TrackedTargetRosterDigest, build_shadow_core_support_digest,
    render_shadow_core_support_summary_section, render_tracked_target_roster_markdown,
};
