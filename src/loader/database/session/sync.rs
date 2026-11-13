//! Bring a loaded database up to date with the files on disk.
//!
//! A long-lived session keeps one database across runs. Between runs the files
//! change; this finds which, feeds them in as input changes so Salsa recomputes
//! only what read them, and says when it cannot: a change to something the
//! loaded Cargo graph was built from, or a source file added or removed, needs
//! a reload instead.

use super::{SourceDatabase, inputs};
use crate::{CordialError, CordialResult};
use ra_ap_hir::ChangeWithProcMacros;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use ra_ap_vfs::VfsPath;
use std::path::{Path, PathBuf};
use tracing::instrument;

/// What bringing the database up to date did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    /// Nothing the database read has changed.
    Current,
    /// These source files changed on disk and were fed in as input changes.
    Updated(Vec<PathBuf>),
    /// The loaded workspace no longer matches the disk and must be reloaded;
    /// the path is the first input that shows it.
    Reload(PathBuf),
}

/// Directory names never searched for source files.
const SKIPPED: &[&str] = &["target", ".git", "node_modules"];

impl SourceDatabase {
    /// Re-read the local packages' source files and apply what changed.
    ///
    /// Only plain source edits are applied in place. A changed manifest, build
    /// script or proc-macro input, a file that vanished, or a new `.rs` file the
    /// database never loaded returns [`SyncOutcome::Reload`] and changes nothing.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn sync_from_disk(&mut self) -> CordialResult<SyncOutcome> {
        let context = self.workspace.clone().ok_or_else(|| {
            CordialError::workspace_source_facts_missing("semantic_workspace_context")
        })?;
        if let Some(path) = inputs::first_stale(&context)? {
            return Ok(SyncOutcome::Reload(path));
        }
        let cargo_home = cargo_home();
        let roots: Vec<PathBuf> = context
            .packages()
            .iter()
            .chain(&self.external_packages)
            .map(|package| package.root().clone())
            .filter(|root| {
                !cargo_home
                    .as_ref()
                    .is_some_and(|home| root.starts_with(home))
            })
            .collect();
        let mut changes = Vec::new();
        for root in &roots {
            let walk = walkdir::WalkDir::new(root)
                .into_iter()
                .filter_entry(|entry| {
                    !(entry.file_type().is_dir()
                        && SKIPPED.iter().any(|name| entry.file_name() == *name))
                });
            for entry in walk.filter_map(Result::ok) {
                let path = entry.path();
                if !entry.file_type().is_file() || path.extension() != Some("rs".as_ref()) {
                    continue;
                }
                let Some(file) = self.loaded_file(path) else {
                    return Ok(SyncOutcome::Reload(path.to_path_buf()));
                };
                let text = match std::fs::read_to_string(path) {
                    Ok(text) => text,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(SyncOutcome::Reload(path.to_path_buf()));
                    }
                    Err(error) => return Err(error.into()),
                };
                if self.db.file_text(file).text(&self.db).as_ref() != text.as_str() {
                    changes.push((path.to_path_buf(), file, text));
                }
            }
        }
        if changes.is_empty() {
            return Ok(SyncOutcome::Current);
        }
        let mut change = ChangeWithProcMacros::default();
        let mut changed = Vec::with_capacity(changes.len());
        for (path, file, text) in changes {
            change.change_file(file, Some(text));
            changed.push(path);
        }
        change.apply(&mut self.db);
        self.revision += 1;
        Ok(SyncOutcome::Updated(changed))
    }

    /// The database's file for `path`, if it loaded one.
    #[instrument(level = "trace", skip(self, path))]
    fn loaded_file(&self, path: &Path) -> Option<ra_ap_vfs::FileId> {
        let text = path.to_str()?;
        self.vfs
            .file_id(&VfsPath::new_real_path(text.to_owned()))
            .map(|(file, _excluded)| file)
    }
}

/// Where Cargo keeps fetched sources, which do not change under a session.
#[instrument(level = "trace")]
fn cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
}
