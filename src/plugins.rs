//! Built-in plugin registrations.

#[cfg(feature = "_etiquette_plugins")]
use std::sync::LazyLock;
use tracing::instrument;
#[cfg(feature = "elicitation")]
mod elicitation;

#[cfg(feature = "homecoming_std")]
mod homecoming;

#[cfg(feature = "amenable_std")]
mod amenable;

#[cfg(feature = "amenable_ext")]
mod amenable_ext;

#[cfg(feature = "elicitation")]
pub use elicitation::{ELICITATION_COVERAGE, ElicitationCoverage};

#[cfg(feature = "homecoming_std")]
pub use homecoming::{HOMECOMING_STD_COVERAGE, HomecomingStdCoverage};

#[cfg(feature = "amenable_std")]
pub use amenable::{AMENABLE_STD_COVERAGE, AmenableStdCoverage};

#[cfg(feature = "amenable_ext")]
pub use amenable_ext::{AMENABLE_EXT_COVERAGE, AmenableExtCoverage};

#[cfg(any(feature = "error_sites", feature = "panics"))]
mod error_handling;

#[cfg(any(feature = "error_sites", feature = "panics"))]
mod error_handling_plugin_wiring {
    pub use super::error_handling::{
        STANDARD_ERROR_HANDLING, StandardErrorHandling, standard_error_handling_etiquettes,
    };
    use crate::plugin::Plugin;
    use tracing::instrument;

    /// Built-in error-handling plugins.
    #[instrument(level = "debug")]
    pub fn error_handling_plugins() -> Vec<&'static dyn Plugin> {
        vec![&STANDARD_ERROR_HANDLING as &dyn Plugin]
    }

    /// Error-handling plugins only.
    #[instrument(level = "debug")]
    pub fn error_handling_only_plugins() -> Vec<&'static dyn Plugin> {
        error_handling_plugins()
    }
}

#[cfg(any(feature = "error_sites", feature = "panics"))]
pub use error_handling_plugin_wiring::{
    STANDARD_ERROR_HANDLING, StandardErrorHandling, error_handling_only_plugins,
    error_handling_plugins, standard_error_handling_etiquettes,
};

use crate::etiquette::Etiquette;
use crate::plugin::{EtiquettePlugin, Plugin, PluginCategory, plugins_in_category};

#[cfg(feature = "rustdoc")]
mod coverage_targets;

#[cfg(feature = "rustdoc")]
pub use coverage_targets::ir_crate_names_for_coverage_plugins;

/// Built-in quality plugins (source scanners). Includes the error-handling
/// family so panicking APIs and Result-chain analysis run together.
#[instrument(level = "debug")]
pub fn quality_plugins() -> Vec<&'static dyn Plugin> {
    let mut out: Vec<&'static dyn Plugin> = quality_etiquette_plugins()
        .into_iter()
        .map(|plugin| plugin as &dyn Plugin)
        .collect();
    out.extend(handling_plugins());
    out
}

#[cfg(any(feature = "error_sites", feature = "panics"))]
fn handling_plugins() -> Vec<&'static dyn Plugin> {
    error_handling_plugins()
}

#[cfg(not(any(feature = "error_sites", feature = "panics")))]
fn handling_plugins() -> Vec<&'static dyn Plugin> {
    Vec::new()
}

/// Built-in coverage plugins.
#[instrument(level = "debug")]
pub fn coverage_plugins() -> Vec<&'static dyn Plugin> {
    #[cfg(feature = "rustdoc")]
    {
        coverage_targets::coverage_plugins()
    }
    #[cfg(not(feature = "rustdoc"))]
    {
        Vec::new()
    }
}

/// Coverage plugins appropriate for a detected workspace hub.
#[instrument(level = "debug", skip(hub))]
#[cfg(any(feature = "homecoming_std", feature = "impl_coverage"))]
pub fn coverage_plugins_for_hub(hub: crate::plugin::WorkspaceHub) -> Vec<&'static dyn Plugin> {
    coverage_targets::coverage_plugins_for_hub(hub)
}

/// All built-in plugins for the current feature set.
#[instrument(level = "debug")]
pub fn all_plugins() -> Vec<&'static dyn Plugin> {
    let mut out = quality_plugins();
    out.extend(coverage_plugins());
    out
}

/// Quality plugins only.
#[instrument(level = "debug")]
pub fn quality_only_plugins() -> Vec<&'static dyn Plugin> {
    plugins_in_category(&all_plugins(), PluginCategory::Quality)
}

/// Coverage plugins only.
#[instrument(level = "debug")]
pub fn coverage_only_plugins() -> Vec<&'static dyn Plugin> {
    plugins_in_category(&all_plugins(), PluginCategory::Coverage)
}

/// Static quality plugins wrapping each enabled etiquette.
#[instrument(level = "debug")]
fn quality_etiquette_plugins() -> Vec<&'static EtiquettePlugin> {
    let items: [Option<&'static EtiquettePlugin>; 19] = [
        #[cfg(feature = "tracing")]
        Some(tracing_plugin()),
        #[cfg(not(feature = "tracing"))]
        None,
        #[cfg(feature = "allows")]
        Some(allows_plugin()),
        #[cfg(not(feature = "allows"))]
        None,
        #[cfg(feature = "modularity")]
        Some(modularity_plugin()),
        #[cfg(not(feature = "modularity"))]
        None,
        #[cfg(feature = "derives")]
        Some(derives_plugin()),
        #[cfg(not(feature = "derives"))]
        None,
        #[cfg(feature = "antipatterns")]
        Some(antipatterns_plugin()),
        #[cfg(not(feature = "antipatterns"))]
        None,
        #[cfg(feature = "cfg_scatter")]
        Some(cfg_scatter_plugin()),
        #[cfg(not(feature = "cfg_scatter"))]
        None,
        #[cfg(feature = "cfg_hygiene")]
        Some(cfg_hygiene_plugin()),
        #[cfg(not(feature = "cfg_hygiene"))]
        None,
        #[cfg(feature = "visibility")]
        Some(visibility_plugin()),
        #[cfg(not(feature = "visibility"))]
        None,
        #[cfg(feature = "cli_layout")]
        Some(cli_layout_plugin()),
        #[cfg(not(feature = "cli_layout"))]
        None,
        #[cfg(feature = "crate_attrs")]
        Some(crate_attrs_plugin()),
        #[cfg(not(feature = "crate_attrs"))]
        None,
        #[cfg(feature = "doc_warnings")]
        Some(doc_warnings_plugin()),
        #[cfg(not(feature = "doc_warnings"))]
        None,
        #[cfg(feature = "feature_warnings")]
        Some(feature_warnings_plugin()),
        #[cfg(not(feature = "feature_warnings"))]
        None,
        #[cfg(feature = "glob_imports")]
        Some(glob_imports_plugin()),
        #[cfg(not(feature = "glob_imports"))]
        None,
        #[cfg(feature = "inline_tests")]
        Some(inline_tests_plugin()),
        #[cfg(not(feature = "inline_tests"))]
        None,
        #[cfg(feature = "verus_warnings")]
        Some(verus_warnings_plugin()),
        #[cfg(not(feature = "verus_warnings"))]
        None,
        #[cfg(feature = "creusot_diagnostics")]
        Some(creusot_diagnostics_plugin()),
        #[cfg(not(feature = "creusot_diagnostics"))]
        None,
        #[cfg(feature = "dependency_freshness")]
        Some(dependency_freshness_plugin()),
        #[cfg(not(feature = "dependency_freshness"))]
        None,
        #[cfg(feature = "proof_patterns")]
        Some(proof_patterns_plugin()),
        #[cfg(not(feature = "proof_patterns"))]
        None,
        #[cfg(feature = "pageantry")]
        Some(pageantry_plugin()),
        #[cfg(not(feature = "pageantry"))]
        None,
    ];
    items.into_iter().flatten().collect()
}

#[cfg(feature = "_etiquette_plugins")]
macro_rules! etiquette_plugin_fn {
    ($fn_name:ident, $etiquette:expr) => {
        fn $fn_name() -> &'static EtiquettePlugin {
            static PLUGIN: LazyLock<EtiquettePlugin> =
                LazyLock::new(|| EtiquettePlugin::new($etiquette));
            &*PLUGIN
        }
    };
}

#[cfg(feature = "tracing")]
etiquette_plugin_fn!(
    tracing_plugin,
    &*crate::etiquettes::tracing::TRACING_ETIQUETTE
);
#[cfg(feature = "allows")]
etiquette_plugin_fn!(allows_plugin, &*crate::etiquettes::allows::ALLOWS_ETIQUETTE);
#[cfg(feature = "modularity")]
etiquette_plugin_fn!(
    modularity_plugin,
    &*crate::etiquettes::modularity::MODULARITY_ETIQUETTE
);
#[cfg(feature = "derives")]
etiquette_plugin_fn!(
    derives_plugin,
    &*crate::etiquettes::derives::DERIVES_ETIQUETTE
);
#[cfg(feature = "antipatterns")]
etiquette_plugin_fn!(
    antipatterns_plugin,
    &*crate::etiquettes::antipatterns::ANTIPATTERNS_ETIQUETTE
);
#[cfg(feature = "cfg_scatter")]
etiquette_plugin_fn!(
    cfg_scatter_plugin,
    &*crate::etiquettes::CFG_SCATTER_ETIQUETTE
);
#[cfg(feature = "cfg_hygiene")]
etiquette_plugin_fn!(
    cfg_hygiene_plugin,
    &*crate::etiquettes::cfg_hygiene::CFG_HYGIENE_ETIQUETTE
);
#[cfg(feature = "visibility")]
etiquette_plugin_fn!(
    visibility_plugin,
    &*crate::etiquettes::visibility::VISIBILITY_ETIQUETTE
);
#[cfg(feature = "cli_layout")]
etiquette_plugin_fn!(
    cli_layout_plugin,
    &*crate::etiquettes::cli_layout::CLI_LAYOUT_ETIQUETTE
);
#[cfg(feature = "crate_attrs")]
etiquette_plugin_fn!(
    crate_attrs_plugin,
    &*crate::etiquettes::crate_attrs::CRATE_ATTRS_ETIQUETTE
);
#[cfg(feature = "doc_warnings")]
etiquette_plugin_fn!(
    doc_warnings_plugin,
    &*crate::etiquettes::doc_warnings::DOC_WARNINGS_ETIQUETTE
);
#[cfg(feature = "feature_warnings")]
etiquette_plugin_fn!(
    feature_warnings_plugin,
    &*crate::etiquettes::feature_warnings::FEATURE_WARNINGS_ETIQUETTE
);
#[cfg(feature = "glob_imports")]
etiquette_plugin_fn!(
    glob_imports_plugin,
    &*crate::etiquettes::glob_imports::GLOB_IMPORTS_ETIQUETTE
);
#[cfg(feature = "inline_tests")]
etiquette_plugin_fn!(
    inline_tests_plugin,
    &*crate::etiquettes::inline_tests::INLINE_TESTS_ETIQUETTE
);
#[cfg(feature = "verus_warnings")]
etiquette_plugin_fn!(
    verus_warnings_plugin,
    &*crate::etiquettes::verus_warnings::VERUS_WARNINGS_ETIQUETTE
);
#[cfg(feature = "creusot_diagnostics")]
etiquette_plugin_fn!(
    creusot_diagnostics_plugin,
    &*crate::etiquettes::creusot_diagnostics::CREUSOT_DIAGNOSTICS_ETIQUETTE
);
#[cfg(feature = "dependency_freshness")]
etiquette_plugin_fn!(
    dependency_freshness_plugin,
    &*crate::etiquettes::DEPENDENCY_FRESHNESS_ETIQUETTE
);
#[cfg(feature = "proof_patterns")]
etiquette_plugin_fn!(
    proof_patterns_plugin,
    &*crate::etiquettes::PROOF_PATTERNS_ETIQUETTE
);
#[cfg(feature = "pageantry")]
etiquette_plugin_fn!(
    pageantry_plugin,
    &*crate::etiquettes::pageantry::PAGEANTRY_ETIQUETTE
);

/// Legacy etiquette list — flatten of all registered quality + coverage plugins.
#[instrument(level = "debug", skip(session))]
pub fn all_etiquettes_from_plugins(
    session: &dyn crate::SessionView,
) -> Vec<std::sync::Arc<dyn Etiquette>> {
    crate::plugin::etiquettes_from_plugins(&all_plugins(), session)
}
