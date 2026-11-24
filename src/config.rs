//! Layered `cordial.toml` -- the canonical home for etiquette thresholds.
//!
//! Sources, later winning:
//! 1. [`CordialConfig::default`] (graceful fallback if no file exists)
//! 2. `{store_home}/cordial.toml` (`~/.cordial` by default)
//! 3. `{workspace}/cordial.toml`

mod amenable_ext;
mod antipatterns;
mod cfg;
mod checks;
mod configured;
mod finding_policy;
mod gates;
mod hotpath_config;
mod internal_error_chain;
mod lint_paths;
mod lints;
mod modularity;
mod pageantry;
mod perf;
mod tiers;
mod tracing;
mod visibility;
mod worklist;

use std::path::Path;

use ::tracing::instrument;
use config::{Config, File, FileFormat};
use serde::{Deserialize, Serialize};

pub use self::amenable_ext::AmenableExtConfig;
#[cfg(feature = "amenable_ext")]
pub use self::amenable_ext::AmenableExtTargetConfig;
pub use self::antipatterns::{AntipatternsConfig, StaticRefPolicy, StaticRefStrategy};
pub use self::cfg::{CfgHygieneThresholds, CfgScatterThresholds};
pub use self::checks::{
    CrateAttrsThresholds, CreusotDiagnosticsThresholds, DependencyFreshnessThresholds,
    DerivesThresholds, DocWarningsThresholds, FeatureWarningsThresholds,
};
pub use self::configured::TomlConfigured;
pub use self::gates::EtiquetteGate;
pub use self::hotpath_config::HotpathConfig;
pub use self::internal_error_chain::{ErrorFieldStrategy, InternalErrorChainThresholds};
pub use self::lints::{LintLocation, LintPathBase, LintPathPolicy, LintPolicy};
pub use self::modularity::ModularityThresholds;
pub use self::pageantry::PageantryThresholds;
pub use self::perf::PerfConfig;
pub use self::tiers::TierConfig;
pub use self::tracing::{
    TracingBoundaryPolicy, TracingStdioPolicy, TracingSubscriberPolicy, TracingThresholds,
};
pub use self::visibility::VisibilityThresholds;
pub use self::worklist::WorklistSettings;

use crate::session::SessionView;

/// All Cordial settings loaded from `cordial.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct CordialConfig {
    /// How much memory a run may use before it stops itself: a size such as `12G` or `512M`, a
    /// share of the memory available when it starts such as `75%`, or `0` for no limit. The
    /// `--memory-limit` flag and `CORDIAL_MEMORY_LIMIT` take precedence; the default is `75%`.
    #[serde(default)]
    memory_limit: Option<String>,
    /// What the pressure ladder works to when that is less than `memory_limit`, in the same
    /// forms; the run is stopped at the limit and not at this. The `--memory-target` flag and
    /// `CORDIAL_MEMORY_TARGET` take precedence; the default is the limit.
    #[serde(default)]
    memory_target: Option<String>,
    /// How early a run lets go of the scaffolding it builds (syntax trees, macro expansions, span
    /// maps), which it makes again if it is wanted again: the share of the memory budget a process
    /// may hold before it does, as `lean` (15%), `balanced` (30%, the default), `roomy` (50%), `off`
    /// (only under pressure) or a number from 0 to 100. Lean suits a machine with little to spare,
    /// and parses again what is wanted again; roomy suits one with memory to spare. The
    /// `--memory-parsimony` flag and `CORDIAL_MEMORY_PARSIMONY` take precedence.
    #[serde(default)]
    memory_parsimony: Option<String>,
    /// Save what is computed for a function as depending on a fingerprint of its crate's
    /// definition map, so that an edit to a function's body makes the next run compute the
    /// functions of that file and not of the whole crate. A run that computed anything then
    /// computes a sample of what it reused again and says if any of it differs. Off by default
    /// until it has run on enough code to be trusted.
    #[serde(default)]
    reuse_across_edits: bool,
    /// How a cfg context is analysed beside the ordinary build: `copies` adds a copy of each crate the
    /// context changes with files of its own, `flips` writes the context's cfg into the ordinary crates,
    /// reads it from them and writes the ordinary cfg back, so that only what read a flipped atom runs
    /// again (see `docs/planning/contexts-as-flips.md`). `copies` until `flips` has run on enough code.
    #[serde(default)]
    cfg_context_mode: crate::CfgContextMode,
    /// Shared enable and path policy, keyed by etiquette configuration identity.
    #[serde(default)]
    lints: std::collections::BTreeMap<String, LintPolicy>,
    #[serde(default)]
    hotpath: HotpathConfig,
    #[serde(default)]
    perf: PerfConfig,
    #[serde(default)]
    tiers: TierConfig,
    #[serde(default)]
    worklist: WorklistSettings,
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
    feature_warnings: FeatureWarningsThresholds,
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
    internal_error_chain: InternalErrorChainThresholds,
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
    #[serde(default)]
    amenable_ext: AmenableExtConfig,
}

impl CordialConfig {
    /// Whether this etiquette should run for the project.
    ///
    /// `[lints.<id>] enabled` overrides the legacy built-in `[<id>]` switch.
    /// Unconfigured ids, including custom plugin etiquettes, default to on.
    #[instrument(level = "trace", skip(self))]
    pub fn etiquette_enabled(&self, id: &str) -> bool {
        self.lints
            .get(id)
            .and_then(|policy| *policy.enabled())
            .unwrap_or_else(|| self.legacy_etiquette_enabled(id))
    }

    /// Include etiquettes that can be enabled by a narrower rule or path setting.
    #[instrument(level = "trace", skip(self))]
    pub fn etiquette_may_run(&self, id: &str) -> bool {
        self.etiquette_enabled(id) || self.lints.get(id).is_some_and(LintPolicy::has_enables)
    }

    /// Resolve shared policy for one rule using its collected location evidence.
    #[instrument(level = "trace", skip(self, location))]
    pub fn lint_enabled(&self, etiquette: &str, rule: &str, location: LintLocation<'_>) -> bool {
        let default = self.etiquette_enabled(etiquette);
        let legacy_skip = etiquette == "antipatterns"
            && location.file().is_some_and(|file| {
                let Some(root) = location.crate_root() else {
                    return false;
                };
                let absolute = root.join(file);
                self.antipatterns
                    .skip_paths()
                    .get(rule)
                    .is_some_and(|paths| {
                        paths
                            .iter()
                            .any(|path| lint_paths::matches(root, path, &absolute).is_some())
                    })
            });
        self.lints
            .get(etiquette)
            .map_or(default && !legacy_skip, |policy| {
                policy.resolve(default && !legacy_skip, rule, location)
            })
    }

    #[instrument(level = "trace", skip(self))]
    fn legacy_etiquette_enabled(&self, id: &str) -> bool {
        match id {
            "visibility" => self.visibility.enabled(),
            "modularity" => self.modularity.enabled(),
            "cfg_scatter" => self.cfg_scatter.enabled(),
            "cfg_hygiene" => self.cfg_hygiene.enabled(),
            "crate_attrs" => self.crate_attrs.enabled(),
            "doc_warnings" => self.doc_warnings.enabled(),
            "feature_warnings" => self.feature_warnings.enabled(),
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
#[hotpath::measure]
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
#[hotpath::measure]
pub fn load_session_config(session: &dyn SessionView) -> CordialConfig {
    load_cordial_config(session.project_root(), session.store_home())
}

/// Convenience for the visibility etiquette.
#[instrument(level = "info")]
#[hotpath::measure]
pub fn load_visibility_thresholds(
    workspace_root: &Path,
    store_home: &Path,
) -> VisibilityThresholds {
    load_cordial_config(workspace_root, store_home).visibility
}

/// Convenience for the derives etiquette.
#[instrument(level = "info")]
#[hotpath::measure]
pub fn load_derives_thresholds(workspace_root: &Path, store_home: &Path) -> DerivesThresholds {
    load_cordial_config(workspace_root, store_home).derives
}

#[instrument(level = "debug")]
#[hotpath::measure]
pub(super) fn default_true() -> bool {
    true
}

#[instrument(level = "debug", skip(builder, path))]
#[hotpath::measure]
fn add_optional_toml(
    builder: config::ConfigBuilder<config::builder::DefaultState>,
    path: &Path,
) -> config::ConfigBuilder<config::builder::DefaultState> {
    builder.add_source(File::from(path).format(FileFormat::Toml).required(false))
}
