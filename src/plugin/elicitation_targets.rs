//! Elicitation coverage target roster — upstream deps and shadow mirror pairs.

use std::collections::HashSet;

use tracing::instrument;

use crate::error::CordialResult;
use crate::plugin::coverage::{CoverageTarget, TargetProvider};
use crate::session::{RunAll, RunFilter, SessionView};
use crate::targets::discover_crate_targets;

use super::elicitation_tracked_targets::{
    ELICITATION_INTERFACE_SHADOW_CRATES, ELICITATION_TRACKED_TARGETS, ElicitationTrackedTarget,
};

/// One upstream ↔ shadow mirror pair active in the current workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowPair {
    /// Upstream crate this shadow pair tracks.
    pub upstream: String,
    /// Shadow crate that should mirror the upstream.
    pub shadow: String,
}

/// Workspace `elicit_*` mirror members with no entry in the tracked roster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedTargetRosterGap {
    /// Workspace members with no tracked elicitation target.
    pub members_without_tracked_target: Vec<String>,
}

/// Target provider for the elicitation coverage profile.
#[derive(Debug, Default, Clone, Copy)]
pub struct ElicitationTargetProvider;

impl TargetProvider for ElicitationTargetProvider {
    #[instrument(level = "trace", skip(self, session, filter))]
    fn coverage_targets(
        &self,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<CoverageTarget>> {
        let all_members: HashSet<String> = discover_crate_targets(session.project_root(), &RunAll)?
            .into_iter()
            .map(|target| target.crate_name)
            .collect();

        let mut targets = Vec::new();
        let mut seen = HashSet::new();

        for member in &all_members {
            let target = CoverageTarget::workspace_member(member.clone());
            if seen.insert(format!("member:{member}")) {
                targets.push(target);
            }
        }

        for tracked in active_tracked_targets(&all_members) {
            if tracked.elicitation_impl {
                let target = CoverageTarget::upstream_dep(tracked.upstream);
                if seen.insert(format!("upstream:{}", tracked.upstream)) {
                    targets.push(target);
                }
            }
            let target = CoverageTarget::shadow_pair(tracked.upstream, tracked.shadow);
            if seen.insert(format!("shadow:{}:{}", tracked.upstream, tracked.shadow)) {
                targets.push(target);
            }
        }

        Ok(apply_coverage_filter(targets, filter))
    }
}

/// Tracked shadow targets whose mirror crate exists in this workspace.
#[instrument(level = "debug", skip(workspace_members))]
pub fn active_tracked_targets(
    workspace_members: &HashSet<String>,
) -> Vec<&'static ElicitationTrackedTarget> {
    ELICITATION_TRACKED_TARGETS
        .iter()
        .filter(|target| workspace_members.contains(target.shadow))
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
        .map(|target| target.crate_name)
        .collect();
    let pairs: Vec<ShadowPair> = active_tracked_targets(&members)
        .into_iter()
        .map(|target| ShadowPair {
            upstream: target.upstream.to_string(),
            shadow: target.shadow.to_string(),
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

/// Look up a tracked target by upstream crate name.
#[instrument(level = "debug")]
pub fn tracked_target_for_upstream(upstream: &str) -> Option<&'static ElicitationTrackedTarget> {
    ELICITATION_TRACKED_TARGETS
        .iter()
        .find(|target| target.upstream == upstream)
}

/// Look up a tracked target by shadow member crate name.
#[instrument(level = "debug")]
pub fn tracked_target_for_shadow(shadow: &str) -> Option<&'static ElicitationTrackedTarget> {
    ELICITATION_TRACKED_TARGETS
        .iter()
        .find(|target| target.shadow == shadow)
}

/// Returns `true` when `crate_name` is an interface crate rather than an upstream mirror.
#[instrument(level = "trace", ret)]
pub fn is_interface_shadow_crate(crate_name: &str) -> bool {
    ELICITATION_INTERFACE_SHADOW_CRATES.contains(&crate_name)
}

/// Compare workspace members against the single tracked-target list.
#[instrument(level = "debug")]
pub fn compare_tracked_target_roster(workspace_members: &[String]) -> TrackedTargetRosterGap {
    let configured_shadows: HashSet<&str> = ELICITATION_TRACKED_TARGETS
        .iter()
        .map(|target| target.shadow)
        .collect();

    let members_without_tracked_target: Vec<String> = workspace_members
        .iter()
        .filter(|member| {
            !is_interface_shadow_crate(member) && !configured_shadows.contains(member.as_str())
        })
        .cloned()
        .collect();

    TrackedTargetRosterGap {
        members_without_tracked_target,
    }
}

#[instrument(level = "debug", skip(targets, filter))]
fn apply_coverage_filter(
    targets: Vec<CoverageTarget>,
    filter: &dyn RunFilter,
) -> Vec<CoverageTarget> {
    if let Some(name) = filter.crate_name() {
        return targets
            .into_iter()
            .filter(|target| target.matches_crate_filter(name))
            .collect();
    }
    if let Some(names) = filter.crates() {
        return targets
            .into_iter()
            .filter(|target| names.iter().any(|name| target.matches_crate_filter(name)))
            .collect();
    }
    targets
}
