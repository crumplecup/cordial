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
    pub crate_name: String,
    pub source_files: Vec<SourceFileDigest>,
    pub rustdoc_json: Option<String>,
    pub enrichers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFileDigest {
    pub path: String,
    pub sha256: String,
}

impl IrCacheDigest {
    #[instrument(level = "trace")]
    pub fn cache_path(cache_dir: &Path, crate_name: &str) -> PathBuf {
        cache_dir.join(format!("{crate_name}.ir.digests.json"))
    }

    #[instrument(level = "debug", skip(load_views), err(level = "warn"))]
    pub fn compute(
        target: &CrateTarget,
        enricher_ids: &[&str],
        load_views: &HashMap<String, Box<dyn crate::loader::LoadView>>,
    ) -> CordialResult<Self> {
        let source_key = format!("{}:{}", target.crate_name, crate::loader::SourceLoader::ID);
        let source_files = load_views
            .get(&source_key)
            .and_then(|view| view.as_any().downcast_ref::<SourceLoadView>())
            .map(|view| digest_source_files(&view.files))
            .unwrap_or_default();

        let rustdoc_json = {
            #[cfg(feature = "rustdoc")]
            {
                crate::rustdoc_loader::resolve_rustdoc_json(
                    &target.crate_root,
                    &target.crate_name,
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
            crate_name: target.crate_name.clone(),
            source_files,
            rustdoc_json,
            enrichers: enricher_ids.iter().map(|id| (*id).to_string()).collect(),
        })
    }

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

fn digest_source_files(files: &[crate::loader::SourceFile]) -> Vec<SourceFileDigest> {
    let mut digests: Vec<SourceFileDigest> = files
        .iter()
        .map(|file| SourceFileDigest {
            path: file.path.display().to_string(),
            sha256: digest_bytes(file.source.as_bytes()),
        })
        .collect();
    digests.sort_by(|a, b| a.path.cmp(&b.path));
    digests
}

#[cfg(feature = "rustdoc")]
fn digest_file(path: &Path) -> CordialResult<String> {
    let bytes = std::fs::read(path)?;
    Ok(digest_bytes(&bytes))
}

fn digest_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn digest_path_uses_crate_name() {
        let path = IrCacheDigest::cache_path(Path::new("/tmp/cache"), "demo");
        assert_eq!(path, PathBuf::from("/tmp/cache/demo.ir.digests.json"));
    }
}
