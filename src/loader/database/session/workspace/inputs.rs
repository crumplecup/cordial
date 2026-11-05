//! Effectful configuration inputs are captured once by the workspace driver.

use crate::{CordialResult, SemanticCargoOptions, SemanticConfigurationInput};
use ra_ap_project_model::{ProjectWorkspace, ProjectWorkspaceKind, TargetKind};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use tracing::instrument;

#[instrument(level = "debug", skip(workspace, options), err(level = "warn"))]
pub(super) fn configuration(
    workspace: &ProjectWorkspace,
    options: &SemanticCargoOptions,
) -> CordialResult<Vec<SemanticConfigurationInput>> {
    let root: &std::path::Path = workspace.workspace_root().as_ref();
    let mut paths = BTreeSet::new();
    paths.insert(root.join("Cargo.lock"));
    for directory in root.ancestors() {
        for relative in [
            ".cargo/config",
            ".cargo/config.toml",
            "rust-toolchain",
            "rust-toolchain.toml",
        ] {
            paths.insert(directory.join(relative));
        }
    }
    if let Some(home) = environment_value(options, "CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            environment_value(options, "HOME").map(|home| PathBuf::from(home).join(".cargo"))
        })
    {
        paths.insert(home.join("config"));
        paths.insert(home.join("config.toml"));
    }
    if let ProjectWorkspaceKind::Cargo { cargo, .. } = &workspace.kind {
        for package in cargo.packages() {
            paths.insert(AsRef::<std::path::Path>::as_ref(&cargo[package].manifest).to_path_buf());
            for target in &cargo[package].targets {
                if cargo[*target].kind == TargetKind::BuildScript {
                    paths.insert(
                        AsRef::<std::path::Path>::as_ref(&cargo[*target].root).to_path_buf(),
                    );
                }
            }
        }
    }
    let mut snapshots = Vec::new();
    for path in paths {
        match std::fs::read(&path) {
            Ok(bytes) => snapshots.push(SemanticConfigurationInput::new(
                path,
                Some(Sha256::digest(bytes).into()),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                snapshots.push(SemanticConfigurationInput::new(path, None))
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(snapshots)
}

#[instrument(level = "trace", skip(options))]
pub(super) fn environment(options: &SemanticCargoOptions) -> [u8; 32] {
    let mut environment: BTreeMap<_, _> = std::env::vars_os().collect();
    for (key, value) in options.environment() {
        if let Some(value) = value {
            environment.insert(key.into(), value.into());
        } else {
            environment.remove(&std::ffi::OsString::from(key));
        }
    }
    let mut digest = Sha256::new();
    for (key, value) in environment {
        for bytes in [key.as_encoded_bytes(), value.as_encoded_bytes()] {
            digest.update(bytes.len().to_le_bytes());
            digest.update(bytes);
        }
    }
    digest.finalize().into()
}

#[instrument(level = "trace", skip(options))]
fn environment_value(options: &SemanticCargoOptions, key: &str) -> Option<std::ffi::OsString> {
    match options.environment().get(key) {
        Some(value) => value.as_ref().map(Into::into),
        None => std::env::var_os(key),
    }
}

/// Check configuration bytes and absence before using the old crate graph.
#[instrument(level = "trace", skip(context), err(level = "warn"))]
pub(super) fn validate(context: &crate::SemanticWorkspaceContext) -> CordialResult<()> {
    for input in context.configuration_inputs() {
        let current = match std::fs::read(input.path()) {
            Ok(bytes) => Some(Sha256::digest(bytes).into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if &current != input.digest() {
            return Err(crate::CordialError::semantic_context_input(
                input.path().clone(),
            ));
        }
    }
    Ok(())
}
