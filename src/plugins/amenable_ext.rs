//! Amenable ext coverage profile: registry evidence + verifier witnesses
//! over a third-party target crate's inventory (jiff, so far).

use crate::AMENABLE_EXT_JIFF_ETIQUETTE;
use crate::error::CordialResult;

use crate::etiquette::Etiquette;
use crate::framework_std::AMENABLE_EXT_JIFF_UPSTREAM_CRATE;
use crate::plugin::{
    Coverage, CoverageTarget, Plugin, PluginCategory, TargetProvider, TraitRequirement,
};
use crate::session::{RunFilter, SessionView};
use crate::targets::discover_crate_targets;

use tracing::instrument;

/// Registry-backed ext coverage has no single composite trait
/// requirement — same reasoning as `amenable::RegistryRequirement`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AmenableExtRegistryRequirement;

impl TraitRequirement for AmenableExtRegistryRequirement {
    #[instrument(level = "trace", skip(self))]
    fn composite_trait(&self) -> Option<&str> {
        None
    }

    #[instrument(level = "trace", skip(self))]
    fn supertraits(&self) -> &[&str] {
        &[]
    }
}

static AMENABLE_EXT_REGISTRY_REQUIREMENT: AmenableExtRegistryRequirement =
    AmenableExtRegistryRequirement;

/// Target provider for amenable ext registry coverage. `upstream_dep`
/// names the target crate itself (`jiff`); workspace members give the
/// probe an `amenable_ext` IR node to fire on, same shape as
/// `AmenableStdTargetProvider`'s own `std_inventory` + workspace-member
/// rows.
#[derive(Debug, Default, Clone, Copy)]
pub struct AmenableExtTargetProvider;

impl TargetProvider for AmenableExtTargetProvider {
    #[instrument(level = "trace", skip(self, session, filter))]
    fn coverage_targets(
        &self,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<CoverageTarget>> {
        let mut targets = vec![CoverageTarget::upstream_dep(
            AMENABLE_EXT_JIFF_UPSTREAM_CRATE,
        )];
        for member in discover_crate_targets(session.project_root(), filter)? {
            targets.push(CoverageTarget::workspace_member(member.crate_name()));
        }
        Ok(targets)
    }
}

static AMENABLE_EXT_TARGETS: AmenableExtTargetProvider = AmenableExtTargetProvider;

/// Amenable framework ext coverage (registry evidence + witness layers
/// over third-party target crates).
#[derive(Debug, Default, Clone, Copy)]
pub struct AmenableExtCoverage;

impl Plugin for AmenableExtCoverage {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "amenable-ext-coverage"
    }

    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        "Amenable ext coverage"
    }

    #[instrument(level = "trace", skip(self))]
    fn static_etiquettes(&self) -> Vec<&'static dyn Etiquette> {
        vec![&*AMENABLE_EXT_JIFF_ETIQUETTE]
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> PluginCategory {
        PluginCategory::Coverage
    }
}

impl Coverage for AmenableExtCoverage {
    #[instrument(level = "trace", skip(self))]
    fn target_provider(&self) -> &dyn TargetProvider {
        &AMENABLE_EXT_TARGETS
    }

    #[instrument(level = "trace", skip(self))]
    fn trait_requirement(&self) -> &dyn TraitRequirement {
        &AMENABLE_EXT_REGISTRY_REQUIREMENT
    }
}

/// Built-in amenable ext coverage plugin.
pub static AMENABLE_EXT_COVERAGE: AmenableExtCoverage = AmenableExtCoverage;
