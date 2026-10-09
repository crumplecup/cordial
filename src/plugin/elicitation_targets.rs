//! Elicitation coverage target roster — upstream deps and shadow mirror pairs.

use std::collections::HashSet;

use tracing::instrument;

use crate::error::CordialResult;
use crate::session::{RunAll, RunFilter};
use crate::targets::discover_crate_targets;

use super::elicitation_tracked_targets::{ELICITATION_TRACKED_TARGETS, ElicitationTrackedTarget};

#[cfg(feature = "elicitation")]
mod profile;
#[cfg(feature = "elicitation")]
pub use profile::{
    ElicitationTargetProvider, TrackedTargetRosterGap, compare_tracked_target_roster,
    is_interface_shadow_crate, tracked_target_for_upstream,
};

/// One upstream ↔ shadow mirror pair active in the current workspace.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ShadowPair {
    /// Upstream crate this shadow pair tracks.
    upstream: String,
    /// Shadow crate that should mirror the upstream.
    shadow: String,
}

/// Tracked shadow targets whose mirror crate exists in this workspace.
#[instrument(level = "debug", skip(workspace_members))]
pub fn active_tracked_targets(
    workspace_members: &HashSet<String>,
) -> Vec<&'static ElicitationTrackedTarget> {
    ELICITATION_TRACKED_TARGETS
        .iter()
        .filter(|target| workspace_members.contains(target.shadow()))
        .collect()
}

/// Active upstream ↔ shadow pairs for the workspace at `project_root`.
#[instrument(level = "debug", skip(filter), err(level = "warn"))]
pub fn discover_active_shadow_pairs(
    project_root: &std::path::Path,
    filter: &dyn RunFilter,
) -> CordialResult<Vec<ShadowPair>> {
    let members: HashSet<String> = discover_crate_targets(project_root, &RunAll)?
        .into_iter()
        .map(|target| target.crate_name().clone())
        .collect();
    let pairs: Vec<ShadowPair> = active_tracked_targets(&members)
        .into_iter()
        .map(|target| ShadowPair {
            upstream: target.upstream().to_string(),
            shadow: target.shadow().to_string(),
        })
        .collect();
    Ok(filter_shadow_pairs(pairs, filter))
}

#[instrument(level = "debug", skip(pairs, filter))]
fn filter_shadow_pairs(pairs: Vec<ShadowPair>, filter: &dyn RunFilter) -> Vec<ShadowPair> {
    if let Some(name) = filter.crate_name() {
        return pairs
            .into_iter()
            .filter(|pair| pair.upstream == name)
            .collect();
    }
    if let Some(names) = filter.crates() {
        return pairs
            .into_iter()
            .filter(|pair| names.iter().any(|name| *name == pair.upstream))
            .collect();
    }
    pairs
}

/// Look up a tracked target by shadow member crate name.
#[instrument(level = "debug")]
pub fn tracked_target_for_shadow(shadow: &str) -> Option<&'static ElicitationTrackedTarget> {
    ELICITATION_TRACKED_TARGETS
        .iter()
        .find(|target| target.shadow() == shadow)
}
