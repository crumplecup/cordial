//! The single source of truth for which cargo features `amenable
//! dump-registry` must be built with.
//!
//! Two independent consumers each need this list -- `framework_std`'s std/
//! ext coverage assessors, and the antipatterns `ANTIPATTERN-UNNAMED-
//! CONTRACT-BOUND-001` rule's own registry fetch -- and a hand-typed copy
//! per consumer already drifted out of sync once for real: the
//! antipatterns copy was missing `jiff`, so every `amenable_ext`/jiff
//! `ContractRecord`/`ProofRecord` registration was silently absent from
//! that rule's dump, and every correctly-named jiff contract call in
//! `amenable_kani`/`amenable_creusot`/`amenable_verus` misreported as
//! unnamed. One shared constant closes that drift risk for good.
//!
//! `std` and `amenable-ext-jiff` coverage share this one cached dump (see
//! each consumer's own `registry_dump_path`), so the list has to be a
//! superset of every active coverage plugin's needs, not just the one
//! that happens to run first -- `jiff` links `amenable_ext`'s
//! `ExtStandard<T>` registrations into the dump binary alongside
//! `creusot`/`verus`'s own `RustStdStandard<T>` witnesses. Each ext
//! target (jiff, chrono, chrono-tz) adds its own activating feature name here.
/// Cargo features `amenable dump-registry` is built with.
pub const AMENABLE_DUMP_REGISTRY_FEATURES: &str = "creusot,verus,jiff,chrono,chrono-tz";

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use tracing::instrument;
use walkdir::WalkDir;

use crate::error::{CordialError, CordialResult};
use crate::store::StoreLayout;

/// The one cached registry dump every consumer (std/ext coverage and the
/// unnamed-contract-bound rule) reads and refreshes. Two consumers once
/// kept separate paths, and the coverage copy went months stale while the
/// other was refreshed, so rows for newer registrations read `Missing`.
#[instrument(level = "debug", skip(store))]
pub fn registry_dump_path(store: &StoreLayout) -> PathBuf {
    store.cache_dir().join("amenable-registry.dump.json")
}

/// Sidecar recording the feature set a dump was built with.
#[instrument(level = "debug")]
fn features_sidecar(dump_path: &Path) -> PathBuf {
    dump_path.with_extension("features")
}

/// Newest mtime among the workspace's Rust sources and manifests,
/// skipping `target/` and hidden directories.
#[instrument(level = "debug", skip(workspace))]
fn newest_source_mtime(workspace: &Path) -> Option<SystemTime> {
    WalkDir::new(workspace)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            entry.depth() == 0 || (name != "target" && !name.starts_with('.'))
        })
        .filter_map(Result::ok)
        .filter(|entry| {
            (entry.file_type().is_file()
                && matches!(
                    entry.path().extension().and_then(|ext| ext.to_str()),
                    Some("rs" | "toml")
                ))
                || entry.file_name() == "Cargo.lock"
        })
        .filter_map(|entry| entry.metadata().ok()?.modified().ok())
        .max()
}

/// Whether the dump at `dump_path` exists, was built with the current
/// feature set, and is newer than every source file in `workspace`.
#[instrument(level = "debug", skip(workspace))]
pub fn registry_dump_is_fresh(workspace: &Path, dump_path: &Path) -> bool {
    let Ok(dump_mtime) = std::fs::metadata(dump_path).and_then(|meta| meta.modified()) else {
        return false;
    };
    let features_match = std::fs::read_to_string(features_sidecar(dump_path))
        .is_ok_and(|recorded| recorded.trim() == AMENABLE_DUMP_REGISTRY_FEATURES);
    features_match && newest_source_mtime(workspace).is_none_or(|newest| newest <= dump_mtime)
}

/// Run `cargo run -p amenable -- dump-registry` in `workspace`, writing
/// `out_path` and recording the feature set beside it.
#[instrument(level = "info", skip(workspace), err(level = "warn"))]
pub fn run_amenable_dump_registry(workspace: &Path, out_path: &Path) -> CordialResult<()> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let status = Command::new("cargo")
        .current_dir(workspace)
        .arg("run")
        .arg("-p")
        .arg("amenable")
        .arg("--features")
        .arg(AMENABLE_DUMP_REGISTRY_FEATURES)
        .arg("--")
        .arg("dump-registry")
        .arg("--out")
        .arg(out_path)
        .status()
        .map_err(|err| {
            CordialError::invariant(format!("failed to run amenable dump-registry: {err}"))
        })?;

    if !status.success() {
        return Err(CordialError::invariant(format!(
            "amenable dump-registry exited with {status}"
        )));
    }
    if !out_path.is_file() {
        return Err(CordialError::invariant(format!(
            "amenable dump-registry did not write {}",
            out_path.display()
        )));
    }
    std::fs::write(features_sidecar(out_path), AMENABLE_DUMP_REGISTRY_FEATURES)?;
    Ok(())
}
