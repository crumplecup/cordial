//! Frozen Cargo context behind a semantic projection. No backend handles escape.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Feature selection used by both Cargo metadata and build-script collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticFeatures {
    /// Enable every workspace feature.
    All,
    /// Enable these features, optionally excluding the default feature set.
    Selected {
        /// Explicit Cargo feature names.
        features: Vec<String>,
        /// Whether Cargo's default features are disabled.
        no_default_features: bool,
    },
}

impl Default for SemanticFeatures {
    #[tracing::instrument(level = "trace")]
    fn default() -> Self {
        Self::Selected {
            features: Vec::new(),
            no_default_features: false,
        }
    }
}

/// A missing part of workspace loading; never evidence that code is infallible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticContextGap {
    /// Cargo returned partial dependency metadata.
    CargoMetadata,
    /// Build scripts or their Cargo invocation did not finish successfully.
    BuildScripts,
    /// Rust library source could not be loaded completely.
    Sysroot,
    /// The toolchain's proc-macro server could not be started.
    ProcMacroServer,
    /// One proc-macro crate has no usable expander in the loaded database.
    ProcMacro {
        /// Root identifying the affected crate, not a printed type name.
        root: PathBuf,
    },
}

/// Resolved crate configuration, including dependency and build-script effects.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct SemanticCrateContext {
    root: PathBuf,
    edition: String,
    cfg: Vec<String>,
    /// Hash the effective environment instead of publishing its values.
    environment_digest: [u8; 32],
    dependencies: Vec<PathBuf>,
    proc_macro: bool,
}

/// The frozen base loaded by the workspace driver. Virtual source revisions
/// are identified separately by `SemanticSnapshot`; changing Cargo inputs
/// requires a driver reload, not a source-text edit in the old crate graph.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_getters::Getters,
    derive_new::new,
    derive_setters::Setters,
)]
#[new(visibility = "pub(crate)")]
#[setters(generate = false, prefix = "with_")]
pub struct SemanticWorkspaceContext {
    /// Identity of this complete frozen Cargo context.
    #[setters(generate)]
    identity: [u8; 32],
    root: PathBuf,
    features: SemanticFeatures,
    target: Option<String>,
    all_targets: bool,
    test_cfg: bool,
    toolchain: Option<String>,
    sysroot: Option<PathBuf>,
    proc_macro_server: Option<PathBuf>,
    /// Manifests, lockfile and discovered Cargo/toolchain configuration files.
    configuration_inputs: Vec<SemanticConfigurationInput>,
    /// Exact database text inputs, including dependencies and generated files,
    /// at initial load. Source revisions remain snapshot-local, not mutations
    /// to this base identity.
    loaded_sources_digest: [u8; 32],
    environment_digest: [u8; 32],
    crates: Vec<SemanticCrateContext>,
    gaps: Vec<SemanticContextGap>,
}

/// Presence and exact bytes of a Cargo configuration input. Absence matters:
/// creating a new configuration file also requires a workspace reload.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct SemanticConfigurationInput {
    path: PathBuf,
    digest: Option<[u8; 32]>,
}
