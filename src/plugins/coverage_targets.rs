use crate::error::CordialResult;
use crate::plugin::{
    CoverageTarget, Plugin, PluginCategory, plugins_in_category, selected_plugins,
};
use crate::session::{RunFilter, SessionView};
use tracing::instrument;

#[derive(Clone, Copy)]
struct CoveragePluginEntry {
    plugin: &'static dyn Plugin,
    coverage: &'static dyn crate::plugin::Coverage,
    hub: crate::plugin::WorkspaceHub,
}

#[instrument(level = "debug")]
fn coverage_plugin_entries() -> Vec<CoveragePluginEntry> {
    [
        #[cfg(feature = "elicitation")]
        Some(CoveragePluginEntry {
            plugin: &super::elicitation::ELICITATION_COVERAGE,
            coverage: &super::elicitation::ELICITATION_COVERAGE,
            hub: crate::plugin::WorkspaceHub::Elicitation,
        }),
        #[cfg(not(feature = "elicitation"))]
        None,
        #[cfg(feature = "homecoming_std")]
        Some(CoveragePluginEntry {
            plugin: &super::homecoming::HOMECOMING_STD_COVERAGE,
            coverage: &super::homecoming::HOMECOMING_STD_COVERAGE,
            hub: crate::plugin::WorkspaceHub::Homecoming,
        }),
        #[cfg(not(feature = "homecoming_std"))]
        None,
        #[cfg(feature = "amenable_std")]
        Some(CoveragePluginEntry {
            plugin: &super::amenable::AMENABLE_STD_COVERAGE,
            coverage: &super::amenable::AMENABLE_STD_COVERAGE,
            hub: crate::plugin::WorkspaceHub::Amenable,
        }),
        #[cfg(not(feature = "amenable_std"))]
        None,
        #[cfg(feature = "amenable_ext")]
        Some(CoveragePluginEntry {
            plugin: &super::amenable_ext::AMENABLE_EXT_COVERAGE,
            coverage: &super::amenable_ext::AMENABLE_EXT_COVERAGE,
            hub: crate::plugin::WorkspaceHub::Amenable,
        }),
        #[cfg(not(feature = "amenable_ext"))]
        None,
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Union [`CoverageTarget`] rows from active coverage plugins in this run.
#[instrument(level = "debug", skip(plugins, session, filter), err(level = "warn"))]
pub fn coverage_targets_for_plugins(
    plugins: &[&'static dyn Plugin],
    session: &dyn SessionView,
    filter: &dyn RunFilter,
) -> CordialResult<Vec<CoverageTarget>> {
    let active = selected_plugins(plugins, filter.plugins());
    let coverage_plugins = plugins_in_category(&active, PluginCategory::Coverage);
    let mut targets = Vec::new();
    for plugin in coverage_plugins {
        targets.extend(coverage_targets_for_plugin(plugin, session, filter)?);
    }
    Ok(dedupe_coverage_targets(targets))
}

/// Crate names that need per-crate IR for active coverage plugins.
#[instrument(level = "debug", skip(plugins, session, filter), err(level = "warn"))]
pub fn ir_crate_names_for_coverage_plugins(
    plugins: &[&'static dyn Plugin],
    session: &dyn SessionView,
    filter: &dyn RunFilter,
) -> CordialResult<Vec<String>> {
    use std::collections::HashSet;

    let mut names = HashSet::new();
    for target in coverage_targets_for_plugins(plugins, session, filter)? {
        for name in target.ir_crate_names() {
            names.insert(name);
        }
    }
    let mut sorted: Vec<String> = names.into_iter().collect();
    sorted.sort();
    Ok(sorted)
}

#[instrument(level = "debug", skip(plugin, session, filter), err(level = "warn"))]
fn coverage_targets_for_plugin(
    plugin: &dyn Plugin,
    session: &dyn SessionView,
    filter: &dyn RunFilter,
) -> CordialResult<Vec<CoverageTarget>> {
    let id = plugin.id();
    coverage_plugin_entries()
        .into_iter()
        .find(|entry| entry.plugin.id() == id)
        .map_or_else(
            || Ok(Vec::new()),
            |entry| entry.coverage.targets(session, filter),
        )
}

#[instrument(level = "debug", skip(targets))]
fn dedupe_coverage_targets(targets: Vec<CoverageTarget>) -> Vec<CoverageTarget> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for target in targets {
        let key = format!(
            "{:?}:{}:{}",
            target.kind(),
            target.crate_name(),
            target.shadow_crate().as_deref().unwrap_or("")
        );
        if seen.insert(key) {
            out.push(target);
        }
    }
    out
}

/// Built-in coverage plugins.
#[instrument(level = "debug")]
pub fn coverage_plugins() -> Vec<&'static dyn Plugin> {
    coverage_plugin_entries()
        .into_iter()
        .map(|entry| entry.plugin)
        .collect()
}

/// Coverage plugins appropriate for a detected workspace hub.
#[instrument(level = "debug", skip(hub))]
pub fn coverage_plugins_for_hub(hub: crate::plugin::WorkspaceHub) -> Vec<&'static dyn Plugin> {
    if hub == crate::plugin::WorkspaceHub::Unknown {
        return coverage_plugins();
    }
    coverage_plugin_entries()
        .into_iter()
        .filter(|entry| entry.hub == hub)
        .map(|entry| entry.plugin)
        .collect()
}
