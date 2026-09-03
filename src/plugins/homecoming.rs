//! Homecoming std coverage profile: `Code` trait over merged std inventory.

use crate::error::CordialResult;
use crate::etiquette::Etiquette;
use crate::etiquettes::framework_std::HOMECOMING_STD_ETIQUETTE;
use crate::framework_std::{FRAMEWORK_STD_SOURCES, HOMECOMING_TRAIT};
use crate::plugin::{
    Coverage, CoverageTarget, CoverageTargetKind, Plugin, PluginCategory, TargetProvider,
    TraitRequirement,
};
use crate::session::{RunFilter, SessionView};
use crate::targets::discover_crate_targets;

use tracing::instrument;
static HOMECOMING_ETIQUETTES: [&'static dyn Etiquette; 1] = [&HOMECOMING_STD_ETIQUETTE];

/// Single-trait requirement for homecoming std coverage.
#[derive(Debug, Default, Clone, Copy)]
pub struct CodeRequirement;

impl TraitRequirement for CodeRequirement {
    #[instrument(level = "trace", skip(self))]
    fn composite_trait(&self) -> Option<&str> {
        None
    }

    #[instrument(level = "trace", skip(self))]
    fn supertraits(&self) -> &[&str] {
        &[HOMECOMING_TRAIT]
    }
}

static CODE_REQUIREMENT: CodeRequirement = CodeRequirement;

/// Target provider for homecoming std coverage.
#[derive(Debug, Default, Clone, Copy)]
pub struct FrameworkStdTargetProvider;

impl TargetProvider for FrameworkStdTargetProvider {
    #[instrument(level = "trace", skip(self, session, filter))]
    fn coverage_targets(
        &self,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<CoverageTarget>> {
        let mut targets = Vec::new();
        for source in FRAMEWORK_STD_SOURCES {
            targets.push(CoverageTarget {
                kind: CoverageTargetKind::StdInventory,
                crate_name: (*source).to_string(),
                shadow_crate: None,
            });
        }
        for member in discover_crate_targets(session.project_root(), filter)? {
            targets.push(CoverageTarget::workspace_member(member.crate_name()));
        }
        Ok(targets)
    }
}

static FRAMEWORK_STD_TARGETS: FrameworkStdTargetProvider = FrameworkStdTargetProvider;

/// Homecoming framework std coverage (`Code` over std inventory).
#[derive(Debug, Default, Clone, Copy)]
pub struct HomecomingStdCoverage;

impl Plugin for HomecomingStdCoverage {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "homecoming-std-coverage"
    }

    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        "Homecoming std coverage"
    }

    #[instrument(level = "trace", skip(self))]
    fn etiquettes(&self) -> &[&'static dyn Etiquette] {
        &HOMECOMING_ETIQUETTES
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> PluginCategory {
        PluginCategory::Coverage
    }
}

impl Coverage for HomecomingStdCoverage {
    #[instrument(level = "trace", skip(self))]
    fn target_provider(&self) -> &dyn TargetProvider {
        &FRAMEWORK_STD_TARGETS
    }

    #[instrument(level = "trace", skip(self))]
    fn trait_requirement(&self) -> &dyn TraitRequirement {
        &CODE_REQUIREMENT
    }
}

/// Built-in homecoming std coverage plugin.
pub static HOMECOMING_STD_COVERAGE: HomecomingStdCoverage = HomecomingStdCoverage;
