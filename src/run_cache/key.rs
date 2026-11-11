//! What a cached run depends on, and the digest that says whether it still holds.
//!
//! A cached analysis is valid only while every input that shaped it is
//! unchanged, and a stale hit would silently report wrong results. So the inputs
//! are gathered broadly: the Cordial binary, the options, the toolchain, the
//! build environment, and every file under every local package, because a build
//! script, an `include_str!` target or a test fixture can all change what the
//! analysis sees. Each input keeps a label, so a miss can say which one changed.

use crate::CordialResult;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tracing::instrument;

/// Bump when the IR or what the analysis records changes shape.
const SCHEMA: u32 = 1;

/// Directory names never hashed: build output and version control.
const SKIPPED_DIRECTORIES: &[&str] = &["target", ".git", "node_modules"];

/// Marker Cargo writes into every build directory, whatever it is named.
const BUILD_DIRECTORY_MARKER: &str = "CACHEDIR.TAG";

/// Every input of one run, by label.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Inputs(BTreeMap<String, String>);

impl Inputs {
    /// The digest naming this set of inputs.
    #[instrument(level = "debug", skip(self))]
    pub(crate) fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        for (label, value) in &self.0 {
            hasher.update(label.as_bytes());
            hasher.update([0]);
            hasher.update(value.as_bytes());
            hasher.update([0]);
        }
        hex(&hasher.finalize())
    }

    /// Labels whose value differs from `previous`, for a miss explanation.
    #[instrument(level = "debug", skip(self, previous))]
    pub(crate) fn changed_since(&self, previous: &Self) -> Vec<String> {
        let mut changed: Vec<String> = self
            .0
            .iter()
            .filter(|(label, value)| previous.0.get(*label) != Some(*value))
            .map(|(label, _)| label.clone())
            .collect();
        changed.extend(
            previous
                .0
                .keys()
                .filter(|label| !self.0.contains_key(*label))
                .cloned(),
        );
        changed
    }

    #[instrument(level = "debug", skip(self, label, value))]
    fn put(&mut self, label: impl Into<String>, value: impl Into<String>) {
        self.0.insert(label.into(), value.into());
    }
}

/// What the caller supplies; everything else is read from the machine.
#[derive(Debug, derive_new::new)]
pub(crate) struct KeyRequest<'a> {
    /// Root of the project being analysed.
    project_root: &'a Path,
    /// The Cargo options and cfg planning, in a stable text form.
    options: String,
}

/// Gather every input of a run on `request.project_root`.
#[instrument(level = "debug", skip_all, err(level = "warn"))]
#[hotpath::measure]
pub(crate) fn gather(request: &KeyRequest<'_>) -> CordialResult<Inputs> {
    let mut inputs = Inputs::default();
    inputs.put("schema", SCHEMA.to_string());
    inputs.put("cordial", executable_fingerprint());
    inputs.put("options", request.options.clone());
    inputs.put("rustc", rustc_version(request.project_root));
    for (name, value) in std::env::vars().filter(|(name, _)| shapes_the_build(name)) {
        inputs.put(format!("env:{name}"), value);
    }
    let (workspace_root, roots) = local_package_roots(request.project_root)?;
    for file in [
        "Cargo.lock",
        "Cargo.toml",
        "rust-toolchain",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        "cordial.toml",
    ] {
        hash_file(&mut inputs, &workspace_root, &workspace_root.join(file));
    }
    for root in &roots {
        hash_tree(&mut inputs, &workspace_root, root);
    }
    Ok(inputs)
}

/// Environment variable names, or prefixes, that change what Cargo and rustc
/// build. Anything else (`RUST_LOG`, `RUST_BACKTRACE`, the per-invocation
/// `CARGO_PKG_*` that Cargo sets for a test) cannot change the analysis.
const BUILD_VARIABLES: &[&str] = &[
    "RUSTFLAGS",
    "RUSTDOCFLAGS",
    "RUSTC",
    "RUSTUP_TOOLCHAIN",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_",
    "CARGO_TARGET_",
    "CARGO_PROFILE_",
    "CARGO_CFG_",
    "CARGO_HOME",
    "CARGO_NET_OFFLINE",
];

#[instrument(level = "debug")]
fn shapes_the_build(name: &str) -> bool {
    BUILD_VARIABLES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Path, size and modification time of this binary: a rebuild changes what the
/// analysis does, so it must change the key.
#[instrument(level = "debug")]
fn executable_fingerprint() -> String {
    std::env::current_exe()
        .and_then(|path| {
            let meta = std::fs::metadata(&path)?;
            let modified = meta
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos());
            Ok(format!("{}:{}:{modified}", path.display(), meta.len()))
        })
        .unwrap_or_else(|error| format!("unknown:{error}"))
}

#[instrument(level = "debug")]
fn rustc_version(project_root: &Path) -> String {
    std::process::Command::new("rustc")
        .arg("-vV")
        .current_dir(project_root)
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_else(|error| format!("unknown:{error}"))
}

/// The workspace root and the root of every package that is not fetched from a
/// registry: workspace members and path dependencies, wherever they live.
#[instrument(level = "debug", err(level = "warn"))]
#[hotpath::measure]
fn local_package_roots(project_root: &Path) -> CordialResult<(PathBuf, Vec<PathBuf>)> {
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--offline", "--format-version", "1"])
        .current_dir(project_root)
        .output()?;
    if !output.status.success() {
        return Err(crate::CordialError::not_found(project_root.to_path_buf()));
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let workspace_root = metadata["workspace_root"]
        .as_str()
        .map_or_else(|| project_root.to_path_buf(), PathBuf::from);
    let mut roots: Vec<PathBuf> = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|package| package["source"].is_null())
        .filter_map(|package| package["manifest_path"].as_str())
        .filter_map(|manifest| Path::new(manifest).parent().map(Path::to_path_buf))
        .collect();
    roots.sort();
    roots.dedup();
    Ok((workspace_root, roots))
}

#[instrument(level = "debug", skip(inputs))]
fn hash_tree(inputs: &mut Inputs, workspace_root: &Path, root: &Path) {
    let entries = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            !(entry.file_type().is_dir()
                && (SKIPPED_DIRECTORIES
                    .iter()
                    .any(|name| entry.file_name() == *name)
                    || entry.path().join(BUILD_DIRECTORY_MARKER).exists()))
        });
    for entry in entries.filter_map(Result::ok) {
        if entry.file_type().is_file() {
            hash_file(inputs, workspace_root, entry.path());
        }
    }
}

#[instrument(level = "debug", skip(inputs, path))]
fn hash_file(inputs: &mut Inputs, workspace_root: &Path, path: &Path) {
    let label = path
        .strip_prefix(workspace_root)
        .unwrap_or(path)
        .display()
        .to_string();
    // A file that cannot be read still changes the key, by the reason.
    let value = match std::fs::read(path) {
        Ok(bytes) => hex(&Sha256::digest(&bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => format!("unreadable:{error}"),
    };
    inputs.put(format!("file:{label}"), value);
}

#[instrument(level = "debug")]
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
