//! Canonical source inputs and semantic queries for the shared source loader.

#[cfg(feature = "cli_layout")]
mod cli_layout;
#[cfg(feature = "error_chain")]
mod error_chains;
#[cfg(feature = "internal_error_chain")]
mod error_compliance;
#[cfg(feature = "internal_error_chain")]
mod error_signatures;
#[cfg(feature = "error_sites")]
mod error_sites;
#[cfg(feature = "internal_error_chain")]
mod error_types;
#[cfg(feature = "tracing")]
mod observability;
#[cfg(feature = "derives")]
mod path_inclusion;
mod project;
#[cfg(feature = "visibility")]
mod visibility;

use std::path::Path;

use ra_ap_hir::ChangeWithProcMacros;
use ra_ap_ide_db::RootDatabase;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, load_workspace_at};
use ra_ap_project_model::{CargoConfig, RustLibSource};
use ra_ap_vfs::{Vfs, VfsPath};
use tracing::instrument;

use crate::{CordialError, CordialResult};

/// Canonical source database shared by source collection and semantic queries.
/// Revisions are serial; virtual changes never write to the loaded project.
#[derive(Debug)]
pub struct SourceDatabase {
    db: RootDatabase,
    vfs: Vfs,
    revision: u64,
    #[cfg(feature = "cfg_hygiene")]
    workspace_root: Option<std::path::PathBuf>,
}

impl SourceDatabase {
    /// Create the source-input owner before Cargo context is available.
    /// Existing syntax collectors consume these inputs during their migration.
    #[instrument(level = "debug")]
    pub(crate) fn new() -> Self {
        Self {
            db: RootDatabase::new(None),
            vfs: Vfs::default(),
            revision: 1,
            #[cfg(feature = "cfg_hygiene")]
            workspace_root: None,
        }
    }

    /// Register one source input and return the database-owned text snapshot.
    /// The source session freezes each input until the next analysis run.
    #[instrument(level = "debug", skip(self, path, text), err(level = "warn"))]
    pub(crate) fn capture(&mut self, path: &Path, text: &str) -> CordialResult<String> {
        let path = std::path::absolute(path)?;
        let text_path = path
            .to_str()
            .ok_or_else(|| CordialError::not_found(path.clone()))?;
        let vfs_path = VfsPath::new_real_path(text_path.to_owned());
        self.vfs
            .set_file_contents(vfs_path.clone(), Some(text.as_bytes().to_vec()));
        let (file, _) = self
            .vfs
            .file_id(&vfs_path)
            .ok_or_else(|| CordialError::not_found(path))?;
        self.db.set_file_text(file, text);
        Ok(self.db.file_text(file).text(&self.db).to_string())
    }

    /// Freeze a loader input without parsing it. Repeated reads reuse the
    /// database value, including virtual revisions made by candidate analysis.
    #[cfg(any(feature = "cfg_hygiene", feature = "tracing"))]
    #[instrument(level = "debug", skip(self, path), err(level = "warn"))]
    pub(crate) fn input_text(&mut self, path: &Path) -> CordialResult<Option<String>> {
        use std::io::ErrorKind;
        let absolute = match path.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == ErrorKind::NotFound => std::path::absolute(path)?,
            Err(error) => return Err(error.into()),
        };
        let text_path = absolute
            .to_str()
            .ok_or_else(|| CordialError::not_found(absolute.clone()))?;
        let vfs_path = VfsPath::new_real_path(text_path.to_owned());
        if let Some((file, _excluded)) = self.vfs.file_id(&vfs_path) {
            return Ok(Some(self.db.file_text(file).text(&self.db).to_string()));
        }
        match std::fs::read_to_string(&absolute) {
            Ok(text) => Ok(Some(self.capture(&absolute, &text)?)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Load Cargo metadata, sysroot and source once for semantic collection.
    /// Build-script outputs and proc macros are not loaded yet. Their absence
    /// must remain a gap until workspace configuration migrates here.
    #[instrument(level = "debug", skip(root), err(level = "warn"))]
    #[hotpath::measure]
    pub fn load(root: &Path) -> CordialResult<Self> {
        let config = CargoConfig {
            sysroot: Some(RustLibSource::Discover),
            metadata_extra_args: vec!["--offline".into()],
            ..CargoConfig::default()
        };
        let load_config = LoadCargoConfig {
            load_out_dirs_from_check: false,
            with_proc_macro_server: ProcMacroServerChoice::None,
            prefill_caches: false,
            num_worker_threads: 1,
            proc_macro_processes: 0,
        };
        let (db, vfs, _server) = load_workspace_at(root, &config, &load_config, &|message| {
            tracing::debug!(%message, "semantic workspace load");
        })?;
        Ok(Self {
            db,
            vfs,
            revision: 1,
            #[cfg(feature = "cfg_hygiene")]
            workspace_root: Some(root.to_path_buf()),
        })
    }

    /// Change one already-loaded source input without reloading Cargo or writing
    /// the file. Unsupported/new paths fail before changing database state.
    #[instrument(level = "debug", skip(self, path, text), err(level = "warn"))]
    pub fn change_file(&mut self, path: &Path, text: String) -> CordialResult<()> {
        let file = self.file_id(path)?;
        let mut change = ChangeWithProcMacros::default();
        change.change_file(file, Some(text));
        change.apply(&mut self.db);
        self.revision += 1;
        Ok(())
    }

    #[instrument(level = "trace", skip(self, path), err(level = "warn"))]
    fn file_id(&self, path: &Path) -> CordialResult<ra_ap_vfs::FileId> {
        let path = path.canonicalize()?;
        // VfsPath takes an absolute UTF-8 path. Refuse unsupported filesystem
        // names before crossing the upstream API's assertion boundary.
        let text = path
            .to_str()
            .ok_or_else(|| CordialError::not_found(path.clone()))?;
        let vfs_path = VfsPath::new_real_path(text.to_owned());
        self.vfs
            .file_id(&vfs_path)
            .map(|(id, _excluded)| id)
            .ok_or_else(|| CordialError::not_found(path))
    }
}
