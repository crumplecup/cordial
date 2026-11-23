//! Record the features of the executable that actually produced a profile, and a fingerprint of the
//! analysis engine it carries.

use sha2::{Digest, Sha256};

/// Every file under `directory`, by path, so that the same tree always hashes the same.
fn files_under(directory: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, found);
        } else {
            found.push(path);
        }
    }
}

/// A hash of what rust-analyzer and Salsa are made of here: the vendored crates and the versions of
/// the rest (the lock file). A state of the engine's own work, made by one build of the tool, is
/// the same for another that has this engine, whatever else of the tool changed.
fn engine_fingerprint() -> String {
    let mut files = Vec::new();
    files_under(std::path::Path::new("vendor"), &mut files);
    files.sort();
    let mut hasher = Sha256::new();
    for path in files
        .iter()
        .chain([std::path::PathBuf::from("Cargo.lock")].iter())
    {
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(std::fs::read(path).unwrap_or_default());
    }
    hasher
        .finalize()
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn main() {
    println!("cargo:rerun-if-changed=vendor");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!(
        "cargo:rustc-env=CORDIAL_ENGINE_FINGERPRINT={}",
        engine_fingerprint()
    );
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        && output.status.success()
    {
        println!(
            "cargo:rustc-env=CORDIAL_BUILD_REVISION={}",
            String::from_utf8_lossy(&output.stdout).trim()
        );
    }
    if let Ok(output) = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        && output.status.success()
    {
        println!(
            "cargo:rustc-env=CORDIAL_BUILD_DIRTY={}",
            !output.stdout.is_empty()
        );
    }
    let mut features: Vec<_> = std::env::vars()
        .filter_map(|(key, _)| key.strip_prefix("CARGO_FEATURE_").map(str::to_owned))
        .collect();
    features.sort();
    println!(
        "cargo:rustc-env=CORDIAL_BUILD_FEATURES={}",
        features.join(",")
    );
}
