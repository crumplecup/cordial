//! How much of a context's copy of the workspace could be the ordinary build's own files.
//!
//! A context copies every local file under new ids, so everything rust-analyzer computes for a file
//! is computed again for the copy, though most files have nothing a context switches. A file can
//! be the original's (the context's source root names it, and rust-analyzer already lets one file
//! be in several crates) when its gates are not flipped by the context and nothing in it can read
//! the cfg in a way the gates do not say. A module is found from its parent's source root, so the
//! files above a changed file are copies too: the *spine*.
//!
//! This only counts. It says, for each context, how many of the files would still be copied.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ra_ap_cfg::CfgOptions;
use ra_ap_hir::{Semantics, attach_db};
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use ra_ap_vfs::FileId;
use tracing::instrument;

use super::flips::{Flips, Reach};
use crate::CordialResult;
use crate::loader::database::SourceDatabase;

/// The files of the workspace a context would still copy, and why.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Reuse {
    /// The workspace's files the context copies today.
    pub(super) files: usize,
    /// Files with a gate the context flips.
    pub(super) flipped: usize,
    /// Files that read the cfg some way the gates do not say (`cfg!`, `cfg_attr`), which cannot be
    /// taken to be unchanged.
    pub(super) unsure: usize,
    /// Files above a changed one, which the module tree is found through.
    pub(super) spine: usize,
}

impl Reuse {
    /// How many files the context would copy.
    pub(super) fn copied(&self) -> usize {
        self.flipped + self.unsure + self.spine
    }

    /// How many could be the original's.
    pub(super) fn aliased(&self) -> usize {
        self.files.saturating_sub(self.copied())
    }
}

/// Whether the text of a file reads the cfg other than through a `#[cfg]` attribute.
fn reads_cfg_otherwise(text: &str) -> bool {
    text.contains("cfg!") || text.contains("cfg_attr") || text.contains("include!")
}

impl SourceDatabase {
    /// Count what a context would copy of the files `paths`, given its cfg `pairs`.
    #[instrument(level = "debug", skip(self, paths, pairs), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn measure_reuse(
        &self,
        paths: &[PathBuf],
        pairs: &[(CfgOptions, CfgOptions)],
    ) -> CordialResult<Reuse> {
        let mut reuse = Reuse::default();
        let mut changed: BTreeSet<FileId> = BTreeSet::new();
        for path in paths {
            let original = self.file_id(path)?;
            reuse.files += 1;
            if Flips::of_file(&self.db, original, pairs, Reach::Written).is_flipped() {
                reuse.flipped += 1;
                changed.insert(original);
            } else if reads_cfg_otherwise(self.db.file_text(original).text(&self.db)) {
                reuse.unsure += 1;
                changed.insert(original);
            }
        }
        let spine = attach_db(&self.db, || {
            let sema = Semantics::new(&self.db);
            let mut above: BTreeSet<FileId> = BTreeSet::new();
            for file in &changed {
                for module in sema.file_to_module_defs(*file) {
                    for ancestor in module.path_to_root(&self.db) {
                        let source = ancestor.definition_source(&self.db);
                        above.insert(source.file_id.original_file(&self.db).file_id(&self.db));
                    }
                }
            }
            above
        });
        reuse.spine = spine.difference(&changed).count();
        Ok(reuse)
    }
}
