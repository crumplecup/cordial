//! Plugin registration — products users register with the session.
//!
//! An [`Etiquette`] is a hook bundle; a [`Plugin`] is a runnable product that
//! contributes one or more etiquettes. See [coverage-as-plugin.md](https://github.com/crumplecup/cordial/blob/main/docs/planning/coverage-as-plugin.md).

use std::borrow::Cow;

use tracing::instrument;
#[cfg(feature = "rustdoc")]
mod coverage;
mod error_handling;

#[cfg(any(feature = "elicitation", feature = "shadow"))]
mod elicitation_targets;
#[cfg(any(feature = "elicitation", feature = "shadow"))]
mod elicitation_tracked_targets;

#[cfg(any(feature = "homecoming_std", feature = "impl_coverage"))]
mod workspace_hub;

#[cfg(all(feature = "rustdoc", feature = "impl_coverage"))]
pub use coverage::classify_elicit_complete_gap;
#[cfg(feature = "rustdoc")]
pub use coverage::{
    Coverage, CoverageTarget, CoverageTargetKind, ElicitCompleteRequirement, GapContext,
    TargetProvider, TraitRequirement, WorkspaceMembersTargetProvider,
};
#[cfg(any(feature = "elicitation", feature = "shadow"))]
pub use elicitation_targets::{
    ElicitationTargetProvider, ShadowPair, TrackedTargetRosterGap, active_tracked_targets,
    compare_tracked_target_roster, discover_active_shadow_pairs, is_interface_shadow_crate,
    tracked_target_for_shadow, tracked_target_for_upstream,
};
#[cfg(any(feature = "elicitation", feature = "shadow"))]
pub use elicitation_tracked_targets::{
    ELICITATION_INTERFACE_SHADOW_CRATES, ELICITATION_TRACKED_TARGETS, ElicitationTrackedTarget,
};
pub use error_handling::{
    ErrorHandling, ErrorHandlingLayers, ErrorHandlingPolicy, ErrorScope, ErrorScopeProvider,
    ErrorSurface, StandardErrorHandlingPolicy, WorkspaceMembersErrorScopeProvider,
};
#[cfg(any(feature = "homecoming_std", feature = "impl_coverage"))]
pub use workspace_hub::{WorkspaceHub, detect_workspace_hub, discover_workspace_hub};

use crate::etiquette::Etiquette;

/// Runnable product registered with the session.
///
/// A plugin contributes one or more etiquettes under one product id. The session
/// flattens plugin etiquettes, removes duplicate etiquette ids, and then runs
/// the resulting hook bundles.
pub trait Plugin: Send + Sync {
    /// Stable identifier for command filters and registration deduplication.
    fn id(&self) -> &str;
    /// Human-readable display name for reports and diagnostics.
    fn name(&self) -> &str;

    /// Hook bundles this plugin contributes.
    ///
    /// Return a stable slice. The session deduplicates etiquettes by
    /// [`Etiquette::id`] after all selected plugins have been flattened.
    fn etiquettes(&self) -> &[&'static dyn Etiquette];

    /// Product category used by quality and coverage command routing.
    fn category(&self) -> PluginCategory {
        PluginCategory::Quality
    }
}

/// Whether a plugin participates in coverage analysis or source-quality scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginCategory {
    /// Run source-quality etiquettes, or apply mechanical patches.
    Quality,
    /// Error Handling.
    ErrorHandling,
    /// Run rustdoc coverage etiquettes.
    Coverage,
}

/// Wraps a single etiquette as a quality plugin (id matches the etiquette id).
#[derive(Clone, Copy)]
pub struct EtiquettePlugin(&'static dyn Etiquette);

impl EtiquettePlugin {
    /// Build a quality plugin wrapper for one etiquette.
    pub const fn new(etiquette: &'static dyn Etiquette) -> Self {
        Self(etiquette)
    }
}

impl Plugin for EtiquettePlugin {
    fn id(&self) -> &str {
        self.0.id()
    }

    fn name(&self) -> &str {
        self.0.name()
    }

    fn etiquettes(&self) -> &[&'static dyn Etiquette] {
        std::slice::from_ref(&self.0)
    }

    fn category(&self) -> PluginCategory {
        PluginCategory::Quality
    }
}

/// Named plugin-only family: one id, one category, N etiquettes.
///
/// Use this for quality families that have no extra semantics. Coverage and
/// error-handling products implement [`Plugin`] plus their supertrait by hand
/// instead — see `examples/custom_plugins`.
#[derive(Clone)]
pub struct StaticPlugin {
    /// Stable identifier.
    id: Cow<'static, str>,
    /// Human-readable name.
    name: Cow<'static, str>,
    /// Plugin category this product belongs to.
    category: PluginCategory,
    /// Etiquettes this product contributes.
    etiquettes: &'static [&'static dyn Etiquette],
}

impl StaticPlugin {
    /// Build a static plugin definition from compile-time metadata.
    pub const fn new(
        id: &'static str,
        name: &'static str,
        category: PluginCategory,
        etiquettes: &'static [&'static dyn Etiquette],
    ) -> Self {
        Self {
            id: Cow::Borrowed(id),
            name: Cow::Borrowed(name),
            category,
            etiquettes,
        }
    }

    /// Build a plugin definition from runtime-owned metadata.
    pub fn owned(
        id: impl Into<String>,
        name: impl Into<String>,
        category: PluginCategory,
        etiquettes: &'static [&'static dyn Etiquette],
    ) -> Self {
        Self {
            id: Cow::Owned(id.into()),
            name: Cow::Owned(name.into()),
            category,
            etiquettes,
        }
    }
}

impl Plugin for StaticPlugin {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.id.as_ref()
    }

    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        self.name.as_ref()
    }

    #[instrument(level = "trace", skip(self))]
    fn etiquettes(&self) -> &[&'static dyn Etiquette] {
        self.etiquettes
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> PluginCategory {
        self.category
    }
}

/// Flatten plugins into a deduplicated etiquette list (stable registration order).
#[instrument(level = "debug", skip(plugins))]
pub fn etiquettes_from_plugins(plugins: &[&'static dyn Plugin]) -> Vec<&'static dyn Etiquette> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for plugin in plugins {
        for etiquette in plugin.etiquettes() {
            if seen.insert(etiquette.id()) {
                out.push(*etiquette);
            }
        }
    }
    out
}

/// Select plugins whose ids match `filter`, or all when `filter` is empty.
#[instrument(level = "debug", skip(registered))]
pub fn selected_plugins(
    registered: &[&'static dyn Plugin],
    filter: Option<&[String]>,
) -> Vec<&'static dyn Plugin> {
    match filter {
        Some(ids) => registered
            .iter()
            .copied()
            .filter(|plugin| ids.iter().any(|id| id == plugin.id()))
            .collect(),
        None => registered.to_vec(),
    }
}

/// Select plugins by category.
#[instrument(level = "debug", skip(plugins, category))]
pub fn plugins_in_category(
    plugins: &[&'static dyn Plugin],
    category: PluginCategory,
) -> Vec<&'static dyn Plugin> {
    plugins
        .iter()
        .copied()
        .filter(|plugin| plugin.category() == category)
        .collect()
}
