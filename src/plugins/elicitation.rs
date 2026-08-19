//! Elicitation coverage profile: ElicitComplete + trenchcoat + shadow.

use crate::etiquette::Etiquette;
use crate::etiquettes::impl_coverage::IMPL_COVERAGE_ETIQUETTE;
use crate::etiquettes::shadow::SHADOW_ETIQUETTE;
use crate::etiquettes::trenchcoat::TRENCHCOAT_ETIQUETTE;
use crate::plugin::{
    Coverage, ElicitCompleteRequirement, ElicitationTargetProvider, Plugin, PluginCategory,
    TargetProvider, TraitRequirement,
};

use tracing::instrument;
static ELICITATION_ETIQUETTES: [&'static dyn Etiquette; 3] = [
    &IMPL_COVERAGE_ETIQUETTE,
    &TRENCHCOAT_ETIQUETTE,
    &SHADOW_ETIQUETTE,
];

static WORKSPACE_TARGETS: ElicitationTargetProvider = ElicitationTargetProvider;
static ELICIT_COMPLETE: ElicitCompleteRequirement = ElicitCompleteRequirement;

/// Elicitation trait-impl coverage (`ElicitComplete`, trenchcoat, shadow).
#[derive(Debug, Default, Clone, Copy)]
pub struct ElicitationCoverage;

impl Plugin for ElicitationCoverage {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "elicitation-coverage"
    }

    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        "Elicitation coverage"
    }

    #[instrument(level = "trace", skip(self))]
    fn etiquettes(&self) -> &[&'static dyn Etiquette] {
        &ELICITATION_ETIQUETTES
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> PluginCategory {
        PluginCategory::Coverage
    }
}

impl Coverage for ElicitationCoverage {
    #[instrument(level = "trace", skip(self))]
    fn target_provider(&self) -> &dyn TargetProvider {
        &WORKSPACE_TARGETS
    }

    #[instrument(level = "trace", skip(self))]
    fn trait_requirement(&self) -> &dyn TraitRequirement {
        &ELICIT_COMPLETE
    }
}

/// Built-in elicitation coverage plugin.
pub static ELICITATION_COVERAGE: ElicitationCoverage = ElicitationCoverage;
