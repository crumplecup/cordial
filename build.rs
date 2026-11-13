//! Digest the sources that shape a cached analysis, so the run-cache key can
//! name them instead of the executable. An edit to the transport, the CLI or the
//! performance tooling then leaves the cache valid; an edit to anything else, a
//! new dependency version, or a different feature set does not.

include!("src/run_cache/analysis_scope.rs");
include!("src/run_cache/analysis_digest.rs");

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let sources = analysis_sources_digest(&root.join("src"))?;
    // What the sources are built with also decides what they do: the
    // dependencies (the rust-analyzer crates among them) and the features.
    let mut features: Vec<String> = std::env::vars()
        .filter_map(|(name, _)| name.strip_prefix("CARGO_FEATURE_").map(str::to_owned))
        .collect();
    features.sort();
    let lock = std::fs::read(root.join("Cargo.lock")).unwrap_or_default();
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(sources.as_bytes());
    hasher.update(features.join(",").as_bytes());
    hasher.update(&lock);
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    println!("cargo:rustc-env=CORDIAL_ANALYSIS_DIGEST={digest}");
    Ok(())
}
