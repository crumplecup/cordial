use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::instrument;

use crate::error::{CordialError, CordialResult};

/// True when a nightly toolchain with `cargo` is available for rustdoc JSON output.
#[instrument(level = "debug")]
pub fn nightly_available() -> bool {
    resolve_nightly_cargo_binary().is_some_and(|cargo| {
        std::process::Command::new(&cargo)
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .is_some_and(|version| version.contains("nightly"))
    })
}

/// Run `cargo rustdoc -p <crate> --output-format json` and return the JSON path.
#[instrument(level = "info", fields(crate_name = crate_name), err(level = "warn"))]
pub fn run_cargo_rustdoc(
    workspace_root: &Path,
    crate_name: &str,
    features: &[&str],
) -> CordialResult<PathBuf> {
    let workspace_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    let project_target = workspace_root.join("target");

    let mut cmd = nightly_cargo();
    cmd.current_dir(workspace_root)
        .arg("rustdoc")
        .arg("-p")
        .arg(crate_name);

    if !features.is_empty() {
        cmd.arg("--features").arg(features.join(","));
    }

    cmd.arg("--target-dir")
        .arg(&project_target)
        .arg("--")
        .arg("--output-format")
        .arg("json")
        .arg("-Z")
        .arg("unstable-options");

    tracing::debug!("running cargo rustdoc");
    let status = cmd.status().map_err(CordialError::from)?;

    if !status.success() {
        return Err(CordialError::invariant(format!(
            "cargo rustdoc for {crate_name} exited with {status}"
        )));
    }

    let normalized = crate_name.replace('-', "_");
    let json_path = project_target
        .join("doc")
        .join(format!("{normalized}.json"));

    if !json_path.is_file() {
        return Err(CordialError::invariant(format!(
            "rustdoc JSON not found at {}",
            json_path.display()
        )));
    }

    tracing::debug!(path = %json_path.display(), "rustdoc JSON produced");
    Ok(json_path)
}

#[instrument(level = "debug")]
pub(crate) fn nightly_cargo() -> Command {
    if let Some(cargo) = resolve_nightly_cargo_binary() {
        let mut cmd = Command::new(&cargo);
        if let Some(bin_dir) = cargo.parent() {
            let rustc = bin_dir.join("rustc");
            let rustdoc = bin_dir.join("rustdoc");
            if rustc.is_file() {
                cmd.env("RUSTC", &rustc);
            }
            if rustdoc.is_file() {
                cmd.env("RUSTDOC", &rustdoc);
            }
            let path = std::env::var_os("PATH").map_or_else(
                || bin_dir.as_os_str().to_owned(),
                |existing| {
                    let mut combined = bin_dir.as_os_str().to_owned();
                    combined.push(":");
                    combined.push(existing);
                    combined
                },
            );
            cmd.env("PATH", path);
        }
        cmd
    } else {
        let mut cmd = Command::new("cargo");
        cmd.env("RUSTUP_TOOLCHAIN", "nightly");
        cmd
    }
}

/// Host target triple for the active nightly toolchain (`rustc -vV`).
#[instrument(level = "debug", err(level = "warn"))]
pub(crate) fn nightly_host_target() -> CordialResult<String> {
    let mut cmd = Command::new(resolve_nightly_rustc_binary()?);
    if resolve_nightly_cargo_binary().is_none() {
        cmd.env("RUSTUP_TOOLCHAIN", "nightly");
    }
    let output = cmd.arg("-vV").output().map_err(CordialError::from)?;
    if !output.status.success() {
        return Err(CordialError::invariant(format!(
            "rustc -vV exited with {}",
            output.status
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            CordialError::invariant("rustc -vV output missing host target triple".to_string())
        })
}

fn resolve_nightly_rustc_binary() -> CordialResult<PathBuf> {
    if let Some(cargo) = resolve_nightly_cargo_binary() {
        let rustc = cargo
            .parent()
            .map(|bin_dir| bin_dir.join("rustc"))
            .filter(|path| path.is_file())
            .ok_or_else(|| {
                CordialError::invariant(
                    "nightly toolchain cargo found but rustc binary missing".to_string(),
                )
            })?;
        return Ok(rustc);
    }

    Ok(PathBuf::from("rustc"))
}

#[instrument(level = "debug")]
pub(crate) fn resolve_nightly_cargo_binary() -> Option<PathBuf> {
    let rustup_home = std::env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".rustup")))?;
    let toolchains = rustup_home.join("toolchains");
    let entries = std::fs::read_dir(&toolchains).ok()?;
    let mut dated = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Prefer dated `nightly-YYYY-MM-DD-*` toolchains over the rolling
        // `nightly-x86_64-unknown-linux-gnu` symlink: newer rollings may omit
        // stability metadata in rustdoc JSON that framework std screening needs.
        if name.starts_with("nightly-") && name != "nightly-x86_64-unknown-linux-gnu" {
            let cargo = entry.path().join("bin/cargo");
            if cargo.is_file() {
                dated.push(cargo);
            }
        }
    }
    dated.sort();
    if let Some(cargo) = dated.pop() {
        return Some(cargo);
    }

    let preferred = rustup_home.join("toolchains/nightly-x86_64-unknown-linux-gnu/bin/cargo");
    if preferred.is_file() {
        return Some(preferred);
    }
    None
}
