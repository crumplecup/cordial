//! Layered `cordial.toml` -- the canonical home for etiquette thresholds.
//!
//! Sources, later winning:
//! 1. [`CordialConfig::default`] (graceful fallback if no file exists)
//! 2. `{store_home}/cordial.toml` (`~/.cordial` by default)
//! 3. `{workspace}/cordial.toml`

mod antipatterns;
mod cfg;
mod checks;
mod gates;
mod modularity;
mod pageantry;
mod tracing;
mod visibility;

use std::path::Path;

use ::tracing::instrument;
use config::{Config, File, FileFormat};
use serde::{Deserialize, Serialize};

pub use self::antipatterns::{AntipatternsConfig, StaticRefPolicy, StaticRefStrategy};
pub use self::cfg::{CfgHygieneThresholds, CfgScatterThresholds};
pub use self::checks::{
    CrateAttrsThresholds, CreusotDiagnosticsThresholds, DependencyFreshnessThresholds,
    DerivesThresholds, DocWarningsThresholds,
};
pub use self::gates::EtiquetteGate;
pub use self::modularity::ModularityThresholds;
pub use self::pageantry::PageantryThresholds;
pub use self::tracing::{
    TracingBoundaryPolicy, TracingStdioPolicy, TracingSubscriberPolicy, TracingThresholds,
};
pub use self::visibility::VisibilityThresholds;

use crate::session::SessionView;

/// All etiquette knobs loaded from `cordial.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct CordialConfig {
    #[serde(default)]
    visibility: VisibilityThresholds,
    #[serde(default)]
    modularity: ModularityThresholds,
    #[serde(default)]
    cfg_scatter: CfgScatterThresholds,
    #[serde(default)]
    cfg_hygiene: CfgHygieneThresholds,
    #[serde(default)]
    crate_attrs: CrateAttrsThresholds,
    #[serde(default)]
    doc_warnings: DocWarningsThresholds,
    #[serde(default)]
    creusot_diagnostics: CreusotDiagnosticsThresholds,
    #[serde(default)]
    dependency_freshness: DependencyFreshnessThresholds,
    #[serde(default)]
    tracing: TracingThresholds,
    #[serde(default)]
    derives: DerivesThresholds,
    #[serde(default)]
    panics: EtiquetteGate,
    #[serde(default)]
    allows: EtiquetteGate,
    #[serde(default)]
    error_sites: EtiquetteGate,
    #[serde(default)]
    error_chain: EtiquetteGate,
    #[serde(default)]
    internal_error_chain: EtiquetteGate,
    #[serde(default)]
    foreign_error_types: EtiquetteGate,
    #[serde(default)]
    foreign_error_attenuation: EtiquetteGate,
    #[serde(default)]
    antipatterns: AntipatternsConfig,
    #[serde(default)]
    cli_layout: EtiquetteGate,
    #[serde(default)]
    glob_imports: EtiquetteGate,
    #[serde(default)]
    inline_tests: EtiquetteGate,
    #[serde(default)]
    verus_warnings: EtiquetteGate,
    #[serde(default)]
    proof_patterns: EtiquetteGate,
    #[serde(default)]
    pageantry: PageantryThresholds,
    #[serde(rename = "impl-coverage", default)]
    impl_coverage: EtiquetteGate,
    #[serde(default)]
    trenchcoat: EtiquetteGate,
    #[serde(default)]
    shadow: EtiquetteGate,
    #[serde(rename = "homecoming-std", default)]
    homecoming_std: EtiquetteGate,
    #[serde(rename = "amenable-std", default)]
    amenable_std: EtiquetteGate,
}

impl CordialConfig {
    /// Whether this etiquette should run for the project.
    ///
    /// Unknown ids (custom plugins) stay on. Built-ins read
    /// `[<id>] enabled` from `cordial.toml` (default `true`).
    #[instrument(level = "debug", skip(self))]
    pub fn etiquette_enabled(&self, id: &str) -> bool {
        match id {
            "visibility" => self.visibility.enabled(),
            "modularity" => self.modularity.enabled(),
            "cfg_scatter" => self.cfg_scatter.enabled(),
            "cfg_hygiene" => self.cfg_hygiene.enabled(),
            "crate_attrs" => self.crate_attrs.enabled(),
            "doc_warnings" => self.doc_warnings.enabled(),
            "creusot_diagnostics" => self.creusot_diagnostics.enabled(),
            "dependency_freshness" => self.dependency_freshness.enabled(),
            "tracing" => self.tracing.enabled(),
            "derives" => self.derives.enabled(),
            "panics" => self.panics.enabled(),
            "allows" => self.allows.enabled(),
            "error_sites" => self.error_sites.enabled(),
            "error_chain" => self.error_chain.enabled(),
            "internal_error_chain" => self.internal_error_chain.enabled(),
            "foreign_error_types" => self.foreign_error_types.enabled(),
            "foreign_error_attenuation" => self.foreign_error_attenuation.enabled(),
            "antipatterns" => self.antipatterns.enabled(),
            "cli_layout" => self.cli_layout.enabled(),
            "glob_imports" => self.glob_imports.enabled(),
            "inline_tests" => self.inline_tests.enabled(),
            "verus_warnings" => self.verus_warnings.enabled(),
            "proof_patterns" => self.proof_patterns.enabled(),
            "pageantry" => self.pageantry.enabled(),
            "impl-coverage" => self.impl_coverage.enabled(),
            "trenchcoat" => self.trenchcoat.enabled(),
            "shadow" => self.shadow.enabled(),
            "homecoming-std" => self.homecoming_std.enabled(),
            "amenable-std" => self.amenable_std.enabled(),
            _ => true,
        }
    }
}

/// Load `cordial.toml` from the workspace and `{store_home}/cordial.toml`,
/// layered over [`CordialConfig::default`]. Workspace wins. Missing or
/// unreadable files fall back to `Default` instead of failing the run.
#[instrument(level = "info")]
pub fn load_cordial_config(workspace_root: &Path, store_home: &Path) -> CordialConfig {
    let mut builder = Config::builder();
    if let Ok(defaults) = Config::try_from(&CordialConfig::default()) {
        builder = builder.add_source(defaults);
    }
    builder = add_optional_toml(builder, &store_home.join("cordial.toml"));
    builder = add_optional_toml(builder, &workspace_root.join("cordial.toml"));
    builder
        .build()
        .and_then(|settings| settings.try_deserialize())
        .unwrap_or_default()
}

/// Same as [`load_cordial_config`] using the session's project root and store home.
#[instrument(level = "info", skip(session))]
pub fn load_session_config(session: &dyn SessionView) -> CordialConfig {
    load_cordial_config(session.project_root(), session.store_home())
}

/// Convenience for the visibility etiquette.
#[instrument(level = "info")]
pub fn load_visibility_thresholds(
    workspace_root: &Path,
    store_home: &Path,
) -> VisibilityThresholds {
    load_cordial_config(workspace_root, store_home).visibility
}

/// Convenience for the derives etiquette.
#[instrument(level = "info")]
pub fn load_derives_thresholds(workspace_root: &Path, store_home: &Path) -> DerivesThresholds {
    load_cordial_config(workspace_root, store_home).derives
}

#[instrument(level = "debug")]
pub(super) fn default_true() -> bool {
    true
}

#[instrument(level = "debug", skip(builder, path))]
fn add_optional_toml(
    builder: config::ConfigBuilder<config::builder::DefaultState>,
    path: &Path,
) -> config::ConfigBuilder<config::builder::DefaultState> {
    builder.add_source(File::from(path).format(FileFormat::Toml).required(false))
}
