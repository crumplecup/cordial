//! Coverage family: a named product type that implements [`Coverage`].
//!
//! Copy this shape when the plugin answers "which types satisfy our trait
//! requirement?" Do not hide those methods behind a builder.

use cordial::{
    Coverage, Etiquette, IMPL_COVERAGE_ETIQUETTE, Plugin, PluginCategory, TargetProvider,
    TraitRequirement, WorkspaceMembersTargetProvider,
};

use tracing::instrument;
static WORKSPACE_TARGETS: WorkspaceMembersTargetProvider = WorkspaceMembersTargetProvider;
static DISPLAY_REQUIREMENT: DisplayRequirement = DisplayRequirement;

static ACME_COVERAGE_ETIQUETTES: &[&dyn Etiquette] = &[&IMPL_COVERAGE_ETIQUETTE];

/// One-trait requirement: types that impl `Display` count as covered.
#[derive(Debug, Default, Clone, Copy)]
pub struct DisplayRequirement;

impl TraitRequirement for DisplayRequirement {
    #[instrument(level = "trace", skip(self))]
    fn composite_trait(&self) -> Option<&str> {
        Some("Display")
    }

    #[instrument(level = "trace", skip(self))]
    fn supertraits(&self) -> &[&str] {
        &[]
    }
}

/// Acme API coverage — `Coverage: Plugin` over workspace members.
#[derive(Debug, Default, Clone, Copy)]
pub struct AcmeApiCoverage;

impl Plugin for AcmeApiCoverage {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "acme-api-coverage"
    }

    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        "Acme API coverage"
    }

    #[instrument(level = "trace", skip(self))]
    fn etiquettes(&self) -> &[&'static dyn Etiquette] {
        ACME_COVERAGE_ETIQUETTES
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> PluginCategory {
        PluginCategory::Coverage
    }
}

impl Coverage for AcmeApiCoverage {
    #[instrument(level = "trace", skip(self))]
    fn target_provider(&self) -> &dyn TargetProvider {
        &WORKSPACE_TARGETS
    }

    #[instrument(level = "trace", skip(self))]
    fn trait_requirement(&self) -> &dyn TraitRequirement {
        &DISPLAY_REQUIREMENT
    }
}

/// Built-in instance for session registration.
pub static ACME_API_COVERAGE: AcmeApiCoverage = AcmeApiCoverage;
