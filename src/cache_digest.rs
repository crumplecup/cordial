use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::instrument;

use crate::error::CordialResult;
use crate::loader::{CrateTarget, SourceLoadView};

/// Fingerprints recorded alongside a cached IR graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrCacheDigest {
    crate_name: String,
    source_files: Vec<SourceFileDigest>,
    rustdoc_json: Option<String>,
    enrichers: Vec<String>,
}

/// Fingerprint of crate sources used to invalidate cached IR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new)]
pub struct SourceFileDigest {
    path: String,
    sha256: String,
}

impl IrCacheDigest {
    /// Store path for this digest file.
    #[instrument(level = "debug")]
    pub fn cache_path(cache_dir: &Path, crate_name: &str) -> PathBuf {
        cache_dir.join(format!("{crate_name}.ir.digests.json"))
    }

    /// Fingerprint the current source inputs for cache invalidation.
    #[instrument(level = "debug", skip(target, load_views), err(level = "warn"))]
    pub fn compute(
        target: &CrateTarget,
        enricher_ids: &[&str],
        load_views: &HashMap<String, Box<dyn crate::loader::LoadView>>,
    ) -> CordialResult<Self> {
        let source_key = format!(
            "{}:{}",
            target.crate_name(),
            crate::loader::SourceLoader::ID
        );
        let source_files = load_views
            .get(&source_key)
            .and_then(|view| view.as_any().downcast_ref::<SourceLoadView>())
            .map(|view| digest_source_files(view.files()))
            .unwrap_or_default();

        let rustdoc_json = {
            #[cfg(feature = "rustdoc")]
            {
                crate::rustdoc_loader::resolve_rustdoc_json(
                    target.crate_root(),
                    target.crate_name(),
                    None,
                )
                .ok()
                .and_then(|path| digest_file(&path).ok())
            }
            #[cfg(not(feature = "rustdoc"))]
            {
                None
            }
        };

        Ok(Self {
            crate_name: target.crate_name().clone(),
            source_files,
            rustdoc_json,
            enrichers: enricher_ids.iter().map(|id| (*id).to_string()).collect(),
        })
    }

    /// Serialize this value to `path`.
    #[instrument(level = "debug", skip(self, path), err(level = "warn"))]
    pub fn write(&self, path: &Path) -> CordialResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

#[instrument(level = "debug", skip(files))]
fn digest_source_files(files: &[crate::loader::SourceFile]) -> Vec<SourceFileDigest> {
    let mut digests: Vec<SourceFileDigest> = files
        .iter()
        .map(|file| {
            SourceFileDigest::new(
                file.path().display().to_string(),
                digest_bytes(file.source().as_bytes()),
            )
        })
        .collect();
    digests.sort_by(|a, b| a.path.cmp(&b.path));
    digests
}

#[instrument(level = "debug", skip(path), err(level = "warn"))]
#[cfg(feature = "rustdoc")]
fn digest_file(path: &Path) -> CordialResult<String> {
    let bytes = std::fs::read(path)?;
    Ok(digest_bytes(&bytes))
}

#[instrument(level = "debug")]
fn digest_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
