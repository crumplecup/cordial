//! Cargo inputs to the workspace driver, independent of upstream handles.

use crate::SemanticFeatures;
use std::collections::BTreeMap;

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
    /// Verifier cfg flags (`kani`, `creusot`, ...) to enable in a second analysis
    /// of the workspace's own packages, inside the same database. Code gated
    /// by them is inactive in the ordinary build and has no semantic facts there.
    proof_cfgs: Vec<String>,
}

impl SemanticCargoOptions {
    #[tracing::instrument(level = "debug", skip(self))]
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
