//! Workspace digests derived from coverage findings and inventories.

#[cfg(feature = "elicitation")]
mod shadow_core_support;

#[cfg(feature = "elicitation")]
pub use shadow_core_support::{
    ShadowCoreSupportDigest, ShadowCoreSupportStatus, ShadowCoreSupportSummary,
    TrackedTargetRosterDigest, build_shadow_core_support_digest,
    render_shadow_core_support_summary_section, render_tracked_target_roster_markdown,
};
