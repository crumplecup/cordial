//! Coverage plugin semantics — shared supertrait for trait-impl coverage profiles.

use crate::error::CordialResult;
#[cfg(feature = "impl_coverage")]
use crate::etiquettes::impl_coverage::ImplGapKind;
use crate::loader::CrateTarget;
use crate::plugin::{Plugin, PluginCategory};
use crate::rustdoc::{ELICIT_COMPLETE_SUPERTRAITS, ELICIT_COMPLETE_TRAIT, TraitPrereqs};
use crate::session::{RunFilter, SessionView};
use crate::targets::discover_crate_targets;

/// Which library a coverage target represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageTargetKind {
    WorkspaceMember,
    UpstreamDep,
    ShadowPair,
    StdInventory,
}

/// One built-inventory scope in a coverage run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageTarget {
    pub kind: CoverageTargetKind,
    pub crate_name: String,
    pub shadow_crate: Option<String>,
}

impl CoverageTarget {
    pub fn workspace_member(crate_name: impl Into<String>) -> Self {
        Self {
            kind: CoverageTargetKind::WorkspaceMember,
            crate_name: crate_name.into(),
            shadow_crate: None,
        }
    }

    pub fn upstream_dep(crate_name: impl Into<String>) -> Self {
        Self {
            kind: CoverageTargetKind::UpstreamDep,
            crate_name: crate_name.into(),
            shadow_crate: None,
        }
    }

    pub fn shadow_pair(upstream: impl Into<String>, shadow: impl Into<String>) -> Self {
        Self {
            kind: CoverageTargetKind::ShadowPair,
            crate_name: upstream.into(),
            shadow_crate: Some(shadow.into()),
        }
    }

    pub fn matches_crate_filter(&self, crate_name: &str) -> bool {
        self.crate_name == crate_name || self.shadow_crate.as_deref() == Some(crate_name)
    }

    /// Crate names that need per-crate IR built for this coverage target.
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

/// What impls count as covered for a coverage profile.
pub trait TraitRequirement: Send + Sync {
    fn composite_trait(&self) -> Option<&str>;
    fn supertraits(&self) -> &[&str];
}

/// Elicitation profile: `ElicitComplete` plus its eight supertraits.
#[derive(Debug, Default, Clone, Copy)]
pub struct ElicitCompleteRequirement;

impl TraitRequirement for ElicitCompleteRequirement {
    fn composite_trait(&self) -> Option<&str> {
        Some(ELICIT_COMPLETE_TRAIT)
    }

    fn supertraits(&self) -> &[&str] {
        ELICIT_COMPLETE_SUPERTRAITS
    }
}

/// Inputs for gap classification.
#[derive(Debug, Clone)]
pub struct GapContext {
    pub type_path: String,
    pub prereqs: TraitPrereqs,
}

impl GapContext {
    #[cfg(feature = "impl_coverage")]
    pub fn gap_kind(&self) -> Option<ImplGapKind> {
        classify_elicit_complete_gap(&self.prereqs)
    }
}

/// Discovers [`CoverageTarget`] rows for a profile.
pub trait TargetProvider: Send + Sync {
    fn coverage_targets(
        &self,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<CoverageTarget>>;
}

/// Default provider: one target per workspace member from `cargo metadata`.
#[derive(Debug, Default, Clone, Copy)]
pub struct WorkspaceMembersTargetProvider;

impl TargetProvider for WorkspaceMembersTargetProvider {
    fn coverage_targets(
        &self,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<CoverageTarget>> {
        let members = discover_crate_targets(session.project_root(), filter)?;
        Ok(members
            .into_iter()
            .map(|target: CrateTarget| CoverageTarget::workspace_member(target.crate_name))
            .collect())
    }
}

/// Semantic supertrait: trait-impl coverage over a target library.
pub trait Coverage: Plugin {
    fn target_provider(&self) -> &dyn TargetProvider;
    fn trait_requirement(&self) -> &dyn TraitRequirement;

    fn targets(
        &self,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<CoverageTarget>> {
        self.target_provider().coverage_targets(session, filter)
    }

    #[cfg(feature = "impl_coverage")]
    fn classify_gap(&self, ctx: &GapContext) -> Option<ImplGapKind> {
        classify_elicit_complete_gap(&ctx.prereqs)
    }
    fn category(&self) -> PluginCategory {
        PluginCategory::Coverage
    }
}

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
