//! The machine, toolchain and dependencies a measurement was taken with.

use crate::run_cache::RUN_CACHE_SCHEMA;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use tracing::instrument;

/// The rust-analyzer release cohort this build links; kept equal to the
/// `ra_ap_*` versions in `Cargo.toml` (a test checks it).
pub const RA_AP_VERSION: &str = "0.0.358";

/// What kind of input a difference between two environments is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// The hardware or operating system: times are not attributable to the code.
    Machine,
    /// The toolchain, features or cache format: a change worth explaining.
    Software,
    /// A resolved dependency version.
    Dependency,
}

/// One input that differs between two environments.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct EnvironmentChange {
    #[getter(copy)]
    kind: ChangeKind,
    name: String,
    before: String,
    after: String,
}

/// What a measurement depends on besides the code. Two records can always be
/// compared; this says which inputs differ, so a delta can be attributed to a
/// dependency update or flagged as a different machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct PerfEnvironment {
    cpu_model: String,
    #[getter(copy)]
    cores: u32,
    #[getter(copy)]
    memory_gib: u64,
    os: String,
    rustc: String,
    cargo: String,
    ra_ap_version: String,
    features: Vec<String>,
    #[getter(copy)]
    run_cache_schema: u32,
    /// Resolved dependency versions from `Cargo.lock`, by package name.
    dependencies: BTreeMap<String, String>,
}

/// The first line a command prints, or `unknown` when it does not run.
#[instrument(level = "trace")]
fn tool_version(program: &str) -> String {
    Command::new(program)
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.lines().next().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned())
}

/// A field of a `/proc` file such as `model name` or `MemTotal`.
#[instrument(level = "trace", skip(path))]
fn proc_field(path: &str, name: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()?
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == name).then(|| value.trim().to_owned())
        })
}

/// The cargo features that change how a measured build behaves.
#[instrument(level = "trace")]
fn enabled_features() -> Vec<String> {
    let mut features = Vec::new();
    if cfg!(feature = "hotpath") {
        features.push("hotpath".to_owned());
    }
    if cfg!(feature = "verus_ir") {
        features.push("verus_ir".to_owned());
    }
    if cfg!(feature = "quick") {
        features.push("quick".to_owned());
    }
    if cfg!(debug_assertions) {
        features.push("debug_assertions".to_owned());
    }
    features
}

/// Package name to resolved version(s) from the `Cargo.lock` beside `root`; a
/// package locked at several versions lists them all, comma separated.
#[instrument(level = "debug", skip(root))]
#[hotpath::measure]
fn locked_dependencies(root: &Path) -> BTreeMap<String, String> {
    let mut resolved: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let packages = std::fs::read_to_string(root.join("Cargo.lock"))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| table.get("package")?.as_array().cloned())
        .unwrap_or_default();
    for package in packages {
        if let (Some(name), Some(version)) = (
            package.get("name").and_then(toml::Value::as_str),
            package.get("version").and_then(toml::Value::as_str),
        ) {
            resolved
                .entry(name.to_owned())
                .or_default()
                .push(version.to_owned());
        }
    }
    resolved
        .into_iter()
        .map(|(name, mut versions)| {
            versions.sort();
            (name, versions.join(","))
        })
        .collect()
}

/// A short digest of the given parts.
#[instrument(level = "trace", skip(parts))]
fn digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    hasher.finalize()[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

impl PerfEnvironment {
    /// Read the environment of the current process, with the dependencies the
    /// project at `root` has locked.
    #[instrument(level = "debug", skip(root))]
    #[hotpath::measure]
    pub fn capture(root: &Path) -> Self {
        let memory_kib = proc_field("/proc/meminfo", "MemTotal")
            .and_then(|value| value.split_whitespace().next()?.parse::<u64>().ok())
            .unwrap_or(0);
        Self {
            cpu_model: proc_field("/proc/cpuinfo", "model name")
                .unwrap_or_else(|| "unknown".to_owned()),
            cores: std::thread::available_parallelism()
                .map_or(0, |count| u32::try_from(count.get()).unwrap_or(u32::MAX)),
            memory_gib: memory_kib.div_ceil(1024 * 1024),
            os: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            rustc: tool_version("rustc"),
            cargo: tool_version("cargo"),
            ra_ap_version: RA_AP_VERSION.to_owned(),
            features: enabled_features(),
            run_cache_schema: RUN_CACHE_SCHEMA,
            dependencies: locked_dependencies(root),
        }
    }

    /// A short digest of the hardware and operating system. Equal values mean
    /// the times were taken on the same kind of machine.
    #[instrument(level = "trace", skip(self))]
    pub fn machine(&self) -> String {
        digest(&[
            &self.cpu_model,
            &self.cores.to_string(),
            &self.memory_gib.to_string(),
            &self.os,
        ])
    }

    /// Every input that differs from `earlier`, machine first, then software,
    /// then dependencies, each in name order. Nothing here forbids a comparison;
    /// it says what a change in the numbers might be due to.
    #[instrument(level = "debug", skip(self, earlier))]
    #[hotpath::measure]
    pub fn changes_from(&self, earlier: &Self) -> Vec<EnvironmentChange> {
        let mut changes = Vec::new();
        let mut note = |kind, name: &str, before: String, after: String| {
            if before != after {
                changes.push(EnvironmentChange::new(kind, name.to_owned(), before, after));
            }
        };
        let machine = ChangeKind::Machine;
        note(
            machine,
            "cpu_model",
            earlier.cpu_model.clone(),
            self.cpu_model.clone(),
        );
        note(
            machine,
            "cores",
            earlier.cores.to_string(),
            self.cores.to_string(),
        );
        note(
            machine,
            "memory_gib",
            earlier.memory_gib.to_string(),
            self.memory_gib.to_string(),
        );
        note(machine, "os", earlier.os.clone(), self.os.clone());
        let software = ChangeKind::Software;
        note(software, "rustc", earlier.rustc.clone(), self.rustc.clone());
        note(software, "cargo", earlier.cargo.clone(), self.cargo.clone());
        note(
            software,
            "ra_ap_version",
            earlier.ra_ap_version.clone(),
            self.ra_ap_version.clone(),
        );
        note(
            software,
            "features",
            earlier.features.join(","),
            self.features.join(","),
        );
        note(
            software,
            "run_cache_schema",
            earlier.run_cache_schema.to_string(),
            self.run_cache_schema.to_string(),
        );
        let names: std::collections::BTreeSet<&String> = earlier
            .dependencies
            .keys()
            .chain(self.dependencies.keys())
            .collect();
        for name in names {
            let missing = || "absent".to_owned();
            note(
                ChangeKind::Dependency,
                name,
                earlier
                    .dependencies
                    .get(name)
                    .cloned()
                    .unwrap_or_else(missing),
                self.dependencies.get(name).cloned().unwrap_or_else(missing),
            );
        }
        changes
    }
}
