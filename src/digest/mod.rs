//! Workspace digests derived from coverage findings and inventories.

#[cfg(feature = "elicitation")]
mod shadow_core_support;

#[cfg(feature = "elicitation")]
pub use shadow_core_support::{
    ImplCrateRollup, ShadowCoreSupportDigest, ShadowCoreSupportStatus, ShadowCoreSupportSummary,
    TrackedTargetRosterDigest, build_shadow_core_support_digest, build_shadow_core_support_summary,
    render_shadow_core_support_summary_section, render_tracked_target_roster_markdown,
};
