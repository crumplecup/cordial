//! Cfg declaration inputs captured in the same database as source syntax.

use super::SourcePolicyInventory;
use crate::{CordialResult, FileSnapshot};
use sha2::{Digest, Sha256};
use std::path::Path;

impl SourcePolicyInventory {
    #[tracing::instrument(level = "debug", skip(self, input), err(level = "warn"))]
    pub(crate) fn collect_declarations(
        &mut self,
        crate_root: &Path,
        workspace_root: &Path,
        mut input: impl FnMut(&Path) -> CordialResult<Option<String>>,
    ) -> CordialResult<()> {
        let mut capture = |path: std::path::PathBuf| -> CordialResult<Option<String>> {
            let source = input(&path)?;
            if let Some(source) = &source {
                self.snapshots.push(FileSnapshot::new(
                    path,
                    Sha256::digest(source.as_bytes()).into(),
                ));
            }
            Ok(source)
        };
        let manifest = capture(crate_root.join("Cargo.toml"))?;
        let workspace = capture(workspace_root.join("Cargo.toml"))?;
        let build = capture(crate_root.join("build.rs"))?;
        self.known_cfg_names = crate::etiquettes::cfg_hygiene::declared_names_from_inputs(
            manifest.as_deref(),
            workspace.as_deref(),
            build.as_deref(),
            &crate::config::CfgHygieneThresholds::default(),
        )?
        .into_iter()
        .collect();
        Ok(())
    }
}
