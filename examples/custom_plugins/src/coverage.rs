//! Coverage family: a named product type that implements [`Coverage`].
//!
//! Copy this shape when the plugin answers "which types satisfy our trait
//! requirement?" Do not hide those methods behind a builder.

use cordial::{
    Coverage, Etiquette, IMPL_COVERAGE_ETIQUETTE, Plugin, PluginCategory, TargetProvider,
    TraitRequirement, WorkspaceMembersTargetProvider,
};

static WORKSPACE_TARGETS: WorkspaceMembersTargetProvider = WorkspaceMembersTargetProvider;
static DISPLAY_REQUIREMENT: DisplayRequirement = DisplayRequirement;

static ACME_COVERAGE_ETIQUETTES: &[&dyn Etiquette] = &[&IMPL_COVERAGE_ETIQUETTE];

/// One-trait requirement: types that impl `Display` count as covered.
#[derive(Debug, Default, Clone, Copy)]
pub struct DisplayRequirement;

impl TraitRequirement for DisplayRequirement {
    fn composite_trait(&self) -> Option<&str> {
        Some("Display")
    }

    fn supertraits(&self) -> &[&str] {
        &[]
    }
}

/// Acme API coverage — `Coverage: Plugin` over workspace members.
#[derive(Debug, Default, Clone, Copy)]
pub struct AcmeApiCoverage;

impl Plugin for AcmeApiCoverage {
    fn id(&self) -> &str {
        "acme-api-coverage"
    }

    fn name(&self) -> &str {
        "Acme API coverage"
    }

    fn etiquettes(&self) -> &[&'static dyn Etiquette] {
        ACME_COVERAGE_ETIQUETTES
    }

    fn category(&self) -> PluginCategory {
        PluginCategory::Coverage
    }
}

impl Coverage for AcmeApiCoverage {
    fn target_provider(&self) -> &dyn TargetProvider {
        &WORKSPACE_TARGETS
    }

    fn trait_requirement(&self) -> &dyn TraitRequirement {
        &DISPLAY_REQUIREMENT
    }
}

/// Built-in instance for session registration.
pub static ACME_API_COVERAGE: AcmeApiCoverage = AcmeApiCoverage;
