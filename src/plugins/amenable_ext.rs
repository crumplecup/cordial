//! Amenable ext coverage profile: registry evidence + verifier witnesses
//! over third-party target crates' inventories, as configured by
//! `[[amenable_ext.target]]` (`docs/planning/amenable-ext-targets-config.md`).

use std::sync::Arc;

use crate::config::load_session_config;
use crate::error::CordialResult;

use crate::etiquette::Etiquette;
use crate::etiquettes::{KNOWN_TARGETS, build_ext_etiquette};
use crate::plugin::{
    Coverage, CoverageTarget, Plugin, PluginCategory, TargetProvider, TraitRequirement,
};
use crate::session::{RunFilter, SessionView};
use crate::targets::discover_crate_targets;

use tracing::instrument;

/// Resolve the configured (or default) target names for this session, in
/// declaration order. No `[[amenable_ext.target]]` entries at all falls
/// back to [`KNOWN_TARGETS`]; any entries listed *replace* that default
/// (see `AmenableExtConfig`'s own docs).
#[instrument(level = "trace", skip(session))]
fn resolved_target_names(session: &dyn SessionView) -> Vec<String> {
    let config = load_session_config(session);
    let amenable_ext = config.amenable_ext();
    if amenable_ext.has_configured_targets() {
        amenable_ext
            .enabled_target_names()
            .into_iter()
            .map(str::to_string)
            .collect()
    } else {
        KNOWN_TARGETS
            .iter()
            .map(|target| target.to_string())
            .collect()
    }
}

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
        let mut targets: Vec<CoverageTarget> = resolved_target_names(session)
            .into_iter()
            .map(CoverageTarget::upstream_dep)
            .collect();
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

    #[instrument(level = "trace", skip(self, session))]
    fn etiquettes(&self, session: &dyn SessionView) -> Vec<Arc<dyn Etiquette>> {
        resolved_target_names(session)
            .into_iter()
            .map(|target| Arc::new(build_ext_etiquette(&target)) as Arc<dyn Etiquette>)
            .collect()
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
