//! Loader-owned file-policy evidence, including inactive source syntax.

use crate::loader::SourceLoadView;
use crate::{CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::path::Path;

mod compatibility;
#[cfg(feature = "cfg_hygiene")]
mod declarations;
mod native;

#[cfg(feature = "allows")]
pub use compatibility::scan_allows_source;
#[cfg(feature = "cfg_hygiene")]
pub use compatibility::scan_cfg_hygiene_source;
#[cfg(feature = "cfg_scatter")]
pub use compatibility::scan_cfg_scatter_source;

/// Complete policy collection for a crate's quality scan trees.
pub const ATTR_SOURCE_POLICY_INVENTORY: &str = "source_policy_inventory";

/// Snapshot-bound policy inputs. An empty inventory is explicit evidence;
/// absent or malformed evidence must not establish compliance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, Default)]
pub struct SourcePolicyInventory {
    crate_root: std::path::PathBuf,
    snapshots: Vec<FileSnapshot>,
    #[cfg(feature = "allows")]
    allows: Vec<crate::etiquettes::allows::AllowSiteRecord>,
    #[cfg(feature = "cfg_hygiene")]
    cfg_names: Vec<crate::etiquettes::cfg_hygiene::CfgNameOccurrence>,
    #[cfg(feature = "cfg_hygiene")]
    known_cfg_names: std::collections::BTreeSet<String>,
    #[cfg(feature = "cfg_scatter")]
    cfg_groups: Vec<crate::etiquettes::cfg_scatter::CfgScatterGroup>,
}

impl SourcePolicyInventory {
    /// Collect once while the loader owns the compatibility syntax snapshots.
    #[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourcePolicyInventory")]
    pub(crate) fn collect(source: &SourceLoadView, crate_root: &Path) -> CordialResult<Self> {
        let mut inventory = Self {
            crate_root: crate_root.to_path_buf(),
            ..Self::default()
        };
        for tree in crate::loader::quality_scan_trees(crate_root) {
            let is_source = tree == crate_root.join("src");
            if !is_source && !cfg!(feature = "allows") {
                continue;
            }
            for file in source.parsed_rust_files_under(&tree, "source-policy", |path| {
                (is_source && cfg!(any(feature = "cfg_hygiene", feature = "cfg_scatter")))
                    || !crate::loader::path_has_fixtures(path, crate_root)
            })? {
                inventory.snapshots.push(FileSnapshot::new(
                    file.path().to_path_buf(),
                    file.source_digest(),
                ));
                #[cfg(feature = "allows")]
                if !crate::loader::path_has_fixtures(file.path(), crate_root) {
                    inventory
                        .allows
                        .extend(compatibility::collect_allows(&file, &tree, crate_root)?);
                }
                if is_source {
                    #[cfg(feature = "cfg_hygiene")]
                    inventory
                        .cfg_names
                        .extend(compatibility::collect_cfg_hygiene(
                            &file, &tree, crate_root,
                        )?);
                    #[cfg(feature = "cfg_scatter")]
                    inventory
                        .cfg_groups
                        .extend(compatibility::collect_cfg_scatter(
                            &file, &tree, crate_root,
                        )?);
                }
            }
        }
        #[cfg(feature = "cfg_hygiene")]
        inventory.collect_declarations(crate_root, source.workspace_root(), |path| {
            source.input_text(path)
        })?;
        inventory.sort();
        Ok(inventory)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub(crate) fn sort(&mut self) {
        self.snapshots.sort_by(|a, b| a.path().cmp(b.path()));
        self.snapshots.dedup();
        #[cfg(feature = "allows")]
        self.allows.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.context().cmp(b.context()))
                .then(a.snippet().cmp(b.snippet()))
        });
        #[cfg(feature = "cfg_hygiene")]
        self.cfg_names.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.name().cmp(b.name()))
        });
        #[cfg(feature = "cfg_scatter")]
        self.cfg_groups.sort_by(|a, b| {
            b.non_field_count()
                .cmp(&a.non_field_count())
                .then(a.file().cmp(b.file()))
                .then(a.predicate().cmp(b.predicate()))
        });
    }
}
