//! Cargo inputs to the workspace driver, independent of upstream handles.

use crate::SemanticFeatures;
use std::collections::BTreeMap;

/// How the loader chooses the cfg contexts it analyses beside the ordinary build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CfgPlanning {
    /// Read the workspace's `#[cfg]` predicates and add a context for every clause
    /// the loaded contexts leave unsatisfied, so gated code is analysed somewhere.
    #[default]
    Automatic,
    /// Analyse only the contexts named in [`SemanticCargoOptions::cfg_contexts`].
    Explicit,
}

/// How a cfg context is analysed beside the ordinary build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CfgContextMode {
    /// Add a copy of each crate the context changes, with its own files, and read the context from the copies.
    #[default]
    Copies,
    /// Change the cfg of the workspace's own crates in place, read the context from the same crates and files,
    /// and put the cfg back. What did not read an atom the context flips is not run again.
    Flips,
}

/// One Cargo configuration for a canonical semantic session.
/// These are driver inputs, not an alternative analysis backend.
#[derive(Debug, Clone, Default, derive_getters::Getters, derive_setters::Setters)]
#[setters(prefix = "with_")]
pub struct SemanticCargoOptions {
    /// Select the features used for metadata and build outputs.
    features: SemanticFeatures,
    /// Select a Cargo target triple, or use the toolchain default.
    target: Option<String>,
    /// Include all Cargo targets when collecting build outputs.
    all_targets: bool,
    /// Enable the test cfg in workspace crates.
    test_cfg: bool,
    /// Explicit environment overrides. Values are never published in the IR.
    environment: BTreeMap<String, Option<String>>,
    /// Extra cfg contexts to analyse beside the ordinary build, each enabling its
    /// cfg flags (`kani`, `creusot`, ...) for the workspace's own packages inside
    /// the same database. Code gated by them is inactive in the ordinary build and
    /// has no semantic facts there. Exclusive cfgs belong in separate contexts.
    cfg_contexts: Vec<crate::SemanticContextKey>,
    /// How each context is analysed: from copies of the crates, or by flipping the crates' own cfg.
    cfg_context_mode: CfgContextMode,
    /// Whether further contexts are planned from the workspace's cfg predicates.
    cfg_planning: CfgPlanning,
    /// Save what is computed for a function as depending on a fingerprint of its crate's
    /// definition map, so that an edit that leaves the map as it was (a function's body) makes the
    /// next run compute only what read the edited file, and not every function of the crate. Off
    /// until it has run on enough code to be trusted (see `docs/planning/ra-persistence.md`).
    reuse_across_edits: bool,
    /// Ask Salsa to drop what is over the capacity of the queries that keep the most recently used
    /// (the syntax trees of files and of macro expansions among them), between the chunks of the
    /// priming and at the end of each stage. Off: a run
    /// is one revision, and Salsa drops those values only when asked or at a revision, so every
    /// tree it makes is kept until it ends.
    evicts_between_chunks: bool,
    /// Do not compute what the collectors that read only a file's tree find in each file before the
    /// steps that merge them: each step then asks for it, file by file, and the first to ask
    /// computes it. For the tests of what the pass does.
    skip_file_scan_priming: bool,
    /// Copy every local crate for a context, and not only the ones it changes and what they depend
    /// on. For the tests of what the pass does.
    copy_every_crate: bool,
    /// Give every context its own copy of each crate it changes, and not the one an earlier context
    /// made when the two differ in nothing the crate names. For the tests of what the pass does.
    do_not_share_context_copies: bool,
    /// Do not work out which modules of a crate a context's copy takes from the original (see
    /// `docs/planning/definition-map-sharing.md`). For the tests of what the pass does.
    do_not_share_definition_maps: bool,
}

impl SemanticCargoOptions {
    #[tracing::instrument(level = "debug", skip(self))]
    #[hotpath::measure]
    pub(super) fn cargo_config(&self) -> ra_ap_project_model::CargoConfig {
        use ra_ap_project_model::{CargoConfig, CargoFeatures, RustLibSource};
        let features = match &self.features {
            SemanticFeatures::All => CargoFeatures::All,
            SemanticFeatures::Selected {
                features,
                no_default_features,
            } => CargoFeatures::Selected {
                features: features.clone(),
                no_default_features: *no_default_features,
            },
        };
        CargoConfig {
            features,
            target: self.target.clone(),
            all_targets: self.all_targets,
            set_test: self.test_cfg,
            sysroot: Some(RustLibSource::Discover),
            metadata_extra_args: vec!["--offline".into()],
            extra_args: vec!["--offline".into()],
            extra_env: self.environment.clone().into_iter().collect(),
            ..CargoConfig::default()
        }
    }
}
