//! Coverage plugin semantics — shared supertrait for trait-impl coverage profiles.

use crate::error::CordialResult;
#[cfg(feature = "impl_coverage")]
use crate::etiquettes::impl_coverage::ImplGapKind;
use crate::loader::CrateTarget;
use crate::plugin::{Plugin, PluginCategory};
use crate::rustdoc::{ELICIT_COMPLETE_SUPERTRAITS, ELICIT_COMPLETE_TRAIT, TraitPrereqs};
use crate::session::{RunFilter, SessionView};
use crate::targets::discover_crate_targets;
use tracing::instrument;

/// What impls count as covered for a coverage profile.
pub trait TraitRequirement: Send + Sync {
    /// Composite trait.
    fn composite_trait(&self) -> Option<&str>;
    /// Supertraits.
    fn supertraits(&self) -> &[&str];
}

/// Discovers [`CoverageTarget`] rows for a profile.
pub trait TargetProvider: Send + Sync {
    /// Coverage targets.
    fn coverage_targets(
        &self,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<CoverageTarget>>;
}

/// Semantic supertrait: trait-impl coverage over a target library.
pub trait Coverage: Plugin {
    /// Target provider.
    fn target_provider(&self) -> &dyn TargetProvider;
    /// Trait requirement.
    fn trait_requirement(&self) -> &dyn TraitRequirement;

    /// Targets.
    fn targets(
        &self,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<CoverageTarget>> {
        self.target_provider().coverage_targets(session, filter)
    }

    /// Classify gap.
    #[cfg(feature = "impl_coverage")]
    fn classify_gap(&self, ctx: &GapContext) -> Option<ImplGapKind> {
        classify_elicit_complete_gap(&ctx.prereqs)
    }
    /// Etiquette / lint category this rule belongs to.
    fn category(&self) -> PluginCategory {
        PluginCategory::Coverage
    }
}

/// Which library a coverage target represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageTargetKind {
    /// A member of the analyzed workspace.
    WorkspaceMember,
    /// Upstream Dep.
    UpstreamDep,
    /// Shadow Pair.
    ShadowPair,
    /// Std Inventory.
    StdInventory,
}

/// One built-inventory scope in a coverage run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageTarget {
    /// Kind of coverage inventory this target represents.
    pub kind: CoverageTargetKind,
    /// Cargo package name.
    pub crate_name: String,
    /// Shadow crate that should mirror the target.
    pub shadow_crate: Option<String>,
}

impl CoverageTarget {
    /// Coverage target for a workspace member crate.
    #[instrument(level = "debug", skip(crate_name))]
    pub fn workspace_member(crate_name: impl Into<String>) -> Self {
        Self {
            kind: CoverageTargetKind::WorkspaceMember,
            crate_name: crate_name.into(),
            shadow_crate: None,
        }
    }

    /// Coverage target for an upstream dependency crate.
    #[instrument(level = "debug", skip(crate_name))]
    pub fn upstream_dep(crate_name: impl Into<String>) -> Self {
        Self {
            kind: CoverageTargetKind::UpstreamDep,
            crate_name: crate_name.into(),
            shadow_crate: None,
        }
    }

    /// Coverage target pairing an upstream crate with its shadow.
    #[instrument(level = "debug", skip(upstream, shadow))]
    pub fn shadow_pair(upstream: impl Into<String>, shadow: impl Into<String>) -> Self {
        Self {
            kind: CoverageTargetKind::ShadowPair,
            crate_name: upstream.into(),
            shadow_crate: Some(shadow.into()),
        }
    }

    /// Whether this target includes `crate_name` as member, upstream, or shadow.
    #[instrument(level = "trace", skip(self))]
    pub fn matches_crate_filter(&self, crate_name: &str) -> bool {
        self.crate_name == crate_name || self.shadow_crate.as_deref() == Some(crate_name)
    }

    /// Crate names that need per-crate IR built for this coverage target.
    #[instrument(level = "debug", skip(self))]
    pub fn ir_crate_names(&self) -> Vec<String> {
        match self.kind {
            CoverageTargetKind::StdInventory => Vec::new(),
            CoverageTargetKind::ShadowPair => {
                let mut names = vec![self.crate_name.clone()];
                if let Some(shadow) = &self.shadow_crate {
                    names.push(shadow.clone());
                }
                names
            }
            CoverageTargetKind::WorkspaceMember | CoverageTargetKind::UpstreamDep => {
                vec![self.crate_name.clone()]
            }
        }
    }
}

/// Elicitation profile: `ElicitComplete` plus its eight supertraits.
#[derive(Debug, Default, Clone, Copy)]
pub struct ElicitCompleteRequirement;

impl TraitRequirement for ElicitCompleteRequirement {
    #[instrument(level = "trace", skip(self))]
    fn composite_trait(&self) -> Option<&str> {
        Some(ELICIT_COMPLETE_TRAIT)
    }

    #[instrument(level = "trace", skip(self))]
    fn supertraits(&self) -> &[&str] {
        ELICIT_COMPLETE_SUPERTRAITS
    }
}

/// Inputs for gap classification.
#[derive(Debug, Clone)]
pub struct GapContext {
    /// Qualified type path.
    pub type_path: String,
    /// ElicitComplete prerequisite trait flags.
    pub prereqs: TraitPrereqs,
}

impl GapContext {
    /// Gap kind.
    #[instrument(level = "trace", skip(self))]
    #[cfg(feature = "impl_coverage")]
    pub fn gap_kind(&self) -> Option<ImplGapKind> {
        classify_elicit_complete_gap(&self.prereqs)
    }
}

/// Default provider: one target per workspace member from `cargo metadata`.
#[derive(Debug, Default, Clone, Copy)]
pub struct WorkspaceMembersTargetProvider;

impl TargetProvider for WorkspaceMembersTargetProvider {
    #[instrument(level = "trace", skip(self, session, filter))]
    fn coverage_targets(
        &self,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<CoverageTarget>> {
        let members = discover_crate_targets(session.project_root(), filter)?;
        Ok(members
            .into_iter()
            .map(|target: CrateTarget| CoverageTarget::workspace_member(target.crate_name()))
            .collect())
    }
}

/// Classify elicit complete gap.
#[instrument(level = "debug", skip(prereqs))]
#[cfg(feature = "impl_coverage")]
pub fn classify_elicit_complete_gap(prereqs: &TraitPrereqs) -> Option<ImplGapKind> {
    if prereqs.elicit_complete {
        return None;
    }
    if prereqs.can_be_direct() && prereqs.our_traits_complete() {
        return Some(ImplGapKind::ReadyForElicitComplete);
    }
    if !prereqs.our_traits_complete() {
        return Some(ImplGapKind::MissingOurTraits);
    }
    if !prereqs.can_be_direct() {
        return Some(ImplGapKind::ExternallyBlocked);
    }
    None
}
