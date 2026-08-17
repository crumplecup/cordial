//! Framework std skip / patch lists.

use std::path::Path;

use serde::Deserialize;
use tracing::instrument;

use crate::framework_std::SkipMap;
use crate::store::StoreLayout;

#[derive(Debug, Clone, Deserialize)]
struct SkipEntry {
    path: String,
    reason: String,
}

/// Load framework std skip list from `{store}/patches/{patch_set}.json`.
#[instrument(skip(store), fields(patch_set))]
pub fn load_framework_skip_map(store: &StoreLayout, patch_set: &str) -> SkipMap {
    let candidates = [
        store.root.join("patches").join(format!("{patch_set}.json")),
        store.exceptions_dir().join(format!("{patch_set}.json")),
    ];
    for path in candidates {
        if path.is_file() {
            return load_skip_file(&path);
        }
    }
    SkipMap::new()
}

fn load_skip_file(path: &Path) -> SkipMap {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "failed to read skip list");
            return SkipMap::new();
        }
    };
    let entries: Vec<SkipEntry> = match serde_json::from_slice(&bytes) {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "failed to parse skip list");
            return SkipMap::new();
        }
    };
    entries
        .into_iter()
        .map(|entry| (entry.path, entry.reason))
        .collect()
}
