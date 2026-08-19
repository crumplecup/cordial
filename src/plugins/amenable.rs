//! Amenable std coverage profile: registry evidence + verifier witnesses over std inventory.

use crate::error::CordialResult;
use crate::etiquette::Etiquette;
use crate::etiquettes::framework_std::AMENABLE_STD_ETIQUETTE;
use crate::framework_std::FRAMEWORK_STD_SOURCES;
use crate::plugin::{
    Coverage, CoverageTarget, CoverageTargetKind, Plugin, PluginCategory, TargetProvider,
    TraitRequirement,
};
use crate::session::{RunFilter, SessionView};
use crate::targets::discover_crate_targets;

use tracing::instrument;
static AMENABLE_ETIQUETTES: [&'static dyn Etiquette; 1] = [&AMENABLE_STD_ETIQUETTE];

/// Registry-backed std coverage has no single composite trait requirement.
#[derive(Debug, Default, Clone, Copy)]
pub struct RegistryRequirement;

impl TraitRequirement for RegistryRequirement {
    #[instrument(level = "trace", skip(self))]
    fn composite_trait(&self) -> Option<&str> {
        None
    }

    #[instrument(level = "trace", skip(self))]
    fn supertraits(&self) -> &[&str] {
        &[]
    }
}

static REGISTRY_REQUIREMENT: RegistryRequirement = RegistryRequirement;

/// Target provider for amenable std registry coverage.
#[derive(Debug, Default, Clone, Copy)]
pub struct AmenableStdTargetProvider;

impl TargetProvider for AmenableStdTargetProvider {
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
            targets.push(CoverageTarget::workspace_member(member.crate_name));
        }
        Ok(targets)
    }
}

static AMENABLE_STD_TARGETS: AmenableStdTargetProvider = AmenableStdTargetProvider;

/// Amenable framework std coverage (registry evidence + witness layers).
#[derive(Debug, Default, Clone, Copy)]
pub struct AmenableStdCoverage;

impl Plugin for AmenableStdCoverage {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "amenable-std-coverage"
    }

    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        "Amenable std coverage"
    }

    #[instrument(level = "trace", skip(self))]
    fn etiquettes(&self) -> &[&'static dyn Etiquette] {
        &AMENABLE_ETIQUETTES
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> PluginCategory {
        PluginCategory::Coverage
    }
}

impl Coverage for AmenableStdCoverage {
    #[instrument(level = "trace", skip(self))]
    fn target_provider(&self) -> &dyn TargetProvider {
        &AMENABLE_STD_TARGETS
    }

    #[instrument(level = "trace", skip(self))]
    fn trait_requirement(&self) -> &dyn TraitRequirement {
        &REGISTRY_REQUIREMENT
    }
}

/// Built-in amenable std coverage plugin.
pub static AMENABLE_STD_COVERAGE: AmenableStdCoverage = AmenableStdCoverage;
