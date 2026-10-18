//! Which kinds of plugin a run exercises, so the right reports are written.

use std::sync::Arc;

use tracing::instrument;

use crate::etiquette::Etiquette;
use crate::plugin::{Plugin, PluginCategory, plugins_in_category, selected_plugins};
use crate::session::RunFilter;

#[cfg(any(
    feature = "homecoming_std",
    feature = "amenable_std",
    feature = "elicitation"
))]
#[instrument(level = "info", skip(plugins, filter, etiquettes))]
pub(super) fn run_includes_coverage(
    plugins: &[&'static dyn Plugin],
    filter: &dyn RunFilter,
    etiquettes: &[Arc<dyn Etiquette>],
) -> bool {
    !plugins_in_category(
        &selected_plugins(plugins, filter.plugins()),
        PluginCategory::Coverage,
    )
    .is_empty()
        || etiquettes.iter().any(|etiquette| etiquette.is_coverage())
}

#[instrument(level = "info", skip(plugins, filter, etiquettes))]
#[cfg(feature = "quality")]
pub(super) fn run_includes_quality(
    plugins: &[&'static dyn Plugin],
    filter: &dyn RunFilter,
    etiquettes: &[Arc<dyn Etiquette>],
) -> bool {
    let active = selected_plugins(plugins, filter.plugins());
    !plugins_in_category(&active, PluginCategory::Quality).is_empty()
        || !plugins_in_category(&active, PluginCategory::ErrorHandling).is_empty()
        || etiquettes.iter().any(|etiquette| !etiquette.is_coverage())
}
