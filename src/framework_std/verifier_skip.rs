//! Verifier-scoped skip lists for amenable std registry coverage.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Deserialize;
use tracing::instrument;

use crate::store::StoreLayout;

/// One intentionally-excepted type, scoped to the verifiers it applies to.
#[derive(Debug, Clone)]
pub struct VerifierSkipEntry {
    pub reason: String,
    /// `None` means every verifier is excepted (whole-row skip).
    pub verifiers: Option<HashSet<String>>,
}

impl VerifierSkipEntry {
    #[instrument(level = "debug", skip(self))]
    pub fn covers(&self, verifier: &str) -> bool {
        match &self.verifiers {
            None => true,
            Some(names) => names.contains(verifier),
        }
    }
}

pub type VerifierSkipMap = HashMap<String, VerifierSkipEntry>;

#[derive(Debug, Clone, Deserialize)]
struct RawVerifierSkipEntry {
    path: String,
    reason: String,
    #[serde(default)]
    verifiers: Option<Vec<String>>,
}

/// Load verifier-scoped skip list from `{store}/patches/{patch_set}.json`.
#[instrument(skip(store), fields(patch_set))]
pub fn load_verifier_skip_map(store: &StoreLayout, patch_set: &str) -> VerifierSkipMap {
    let candidates = [
        store.root.join("patches").join(format!("{patch_set}.json")),
        store.exceptions_dir().join(format!("{patch_set}.json")),
    ];
    for path in candidates {
        if path.is_file() {
            return load_verifier_skip_file(&path);
        }
    }
    VerifierSkipMap::new()
}

fn load_verifier_skip_file(path: &Path) -> VerifierSkipMap {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "failed to read verifier skip list");
            return VerifierSkipMap::new();
        }
    };
    let entries: Vec<RawVerifierSkipEntry> = match serde_json::from_slice(&bytes) {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "failed to parse verifier skip list");
            return VerifierSkipMap::new();
        }
    };
    entries
        .into_iter()
        .map(|entry| {
            (
                entry.path,
                VerifierSkipEntry {
                    reason: entry.reason,
                    verifiers: entry.verifiers.map(|names| names.into_iter().collect()),
                },
            )
        })
        .collect()
}
