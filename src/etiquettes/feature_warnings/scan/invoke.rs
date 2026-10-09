//! Run `cargo hack` and find the package's private features.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use super::HackOutput;
use crate::config::FeatureWarningsThresholds;
use crate::error::CordialResult;

use tracing::instrument;

/// The package's private features: those named with a leading `_`. They are
/// only ever enabled by other features, so the powerset skips them as units.
#[instrument(level = "debug")]
fn private_features(crate_root: &Path, crate_name: &str) -> Vec<String> {
    let Ok(metadata) = cargo_metadata::MetadataCommand::new()
        .current_dir(crate_root)
        .no_deps()
        .exec()
    else {
        return Vec::new();
    };
    metadata
        .packages
        .iter()
        .filter(|package| package.name.as_str() == crate_name)
        .flat_map(|package| package.features.keys())
        .filter(|feature| feature.starts_with('_'))
        .cloned()
        .collect()
}

#[instrument(level = "debug")]
fn cargo_command() -> Command {
    let cargo: OsString = std::env::var_os("CORDIAL_CARGO")
        .or_else(|| std::env::var_os("CARGO"))
        .unwrap_or_else(|| OsString::from("cargo"));
    Command::new(cargo)
}

#[instrument(level = "debug")]
pub(super) fn cargo_hack_available(crate_root: &Path) -> bool {
    cargo_command()
        .current_dir(crate_root)
        .args(["hack", "--version"])
        .output()
        .is_ok_and(|output| output.status.success())
}

#[instrument(level = "info", skip(policy), fields(crate_name = crate_name))]
pub(super) fn run_cargo_hack(
    crate_root: &Path,
    crate_name: &str,
    policy: &FeatureWarningsThresholds,
) -> CordialResult<HackOutput> {
    let target_dir = crate_root.join("target").join("feature-warnings");
    let mut command = cargo_command();
    command
        .current_dir(crate_root)
        // Every combination is a cold cache; incremental only costs memory.
        .env("CARGO_INCREMENTAL", "0")
        .args(["hack", "check", "--feature-powerset", "--keep-going"])
        .arg("--depth")
        .arg(policy.depth().to_string())
        .arg("--message-format=json")
        .arg("-p")
        .arg(crate_name)
        .arg("--target-dir")
        .arg(&target_dir);
    let mut excluded: Vec<String> = policy.exclude_features().to_vec();
    excluded.extend(private_features(crate_root, crate_name));
    if !excluded.is_empty() {
        command.arg("--exclude-features").arg(excluded.join(","));
    }
    for group in policy.group_features() {
        command.arg("--group-features").arg(group.join(","));
    }
    let output = command.output()?;
    Ok(HackOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        success: output.status.success(),
    })
}
