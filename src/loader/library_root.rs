//! Resolve library entry paths for source loading and crate-attribute policy.

use std::path::{Path, PathBuf};
use tracing::instrument;

/// Absolute path of this package's library root, if it has one.
#[instrument(level = "debug")]
pub fn library_root_rs(crate_root: &Path) -> Option<PathBuf> {
    let default = crate_root.join("src").join("lib.rs");
    let manifest = crate_root.join("Cargo.toml");
    if manifest.is_file()
        && let Ok(text) = std::fs::read_to_string(&manifest)
        && let Ok(parsed) = toml::from_str::<toml::Value>(&text)
    {
        if let Some(path) = parsed
            .get("lib")
            .and_then(|lib| lib.get("path"))
            .and_then(|path| path.as_str())
        {
            return Some(crate_root.join(path));
        }
        if parsed.get("lib").is_some() {
            return Some(default);
        }
    }
    default.is_file().then_some(default)
}
