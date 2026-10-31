//! Canonical source inputs and semantic queries for the shared source loader.

use super::queries;

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ra_ap_hir::{ChangeWithProcMacros, Semantics, attach_db};
use ra_ap_ide_db::RootDatabase;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, load_workspace_at};
use ra_ap_project_model::{CargoConfig, RustLibSource};
use ra_ap_syntax::{AstNode, ast};
use ra_ap_vfs::{Vfs, VfsPath};
use tracing::instrument;

use crate::{CordialError, CordialResult, CrateIr, EdgeKind, ItemKind, NodeKind, NodeWeight};

/// Canonical source database shared by source collection and semantic queries.
/// Revisions are serial; virtual changes never write to the loaded project.
#[derive(Debug)]
pub struct SourceDatabase {
    db: RootDatabase,
    vfs: Vfs,
    revision: u64,
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

    /// Materialize tracked semantic facts into the ordinary shared graph.
    /// The source selection must belong to one library target and Cargo
    /// configuration. Broader target contexts are part of loader migration.
    #[instrument(level = "debug", skip(self, paths), err(level = "warn"))]
    #[hotpath::measure]
    pub fn project(&self, crate_name: &str, paths: &[PathBuf]) -> CordialResult<CrateIr> {
        attach_db(&self.db, || self.project_attached(crate_name, paths))
    }

    #[instrument(level = "debug", skip(self, paths), err(level = "warn"))]
    fn project_attached(&self, crate_name: &str, paths: &[PathBuf]) -> CordialResult<CrateIr> {
        let mut graph = CrateIr::new(crate_name);
        let mut snapshots = Vec::new();
        let mut definitions = HashMap::new();
        let mut edges = Vec::new();
        let sema = Semantics::new(&self.db);
        for path in paths {
            let file = self.file_id(path)?;
            let text = self.db.file_text(file).text(&self.db);
            snapshots.push(crate::ir::FileSnapshot::new(
                path.canonicalize()?,
                Sha256::digest(text.as_bytes()).into(),
            ));
            let tree = sema.parse_guess_edition(file);
            for syntax in tree.syntax().descendants().filter_map(ast::Fn::cast) {
                let Some(function) = sema.to_def(&syntax) else {
                    // Inactive syntax remains available to syntax collectors.
                    // It must not acquire an active semantic definition.
                    continue;
                };
                let key = queries::FunctionKey::new(&self.db, function);
                let facts = queries::function_evidence(&self.db, key)
                    .as_ref()
                    .ok_or_else(|| {
                        CordialError::workspace_source_facts_missing("semantic function source")
                    })?;
                let mut weight = NodeWeight::new(NodeKind::Item(ItemKind::Fn))
                    .with_name(facts.label().clone())
                    .with_span(crate::FileSpan::new(
                        path,
                        (text[..usize::from(syntax.syntax().text_range().start())]
                            .bytes()
                            .filter(|byte| *byte == b'\n')
                            .count()
                            + 1) as u32,
                        1,
                    ));
                weight.set_attr(crate::ATTR_SEMANTIC_FUNCTION, serde_json::to_value(facts)?);
                let node = graph.insert_node(weight);
                graph.insert_edge(graph.root(), node, EdgeKind::Contains)?;
                definitions.insert(*facts.definition(), node);
                for call in facts.calls() {
                    if let crate::SemanticCallTarget::Function(target) = *call.target() {
                        edges.push((node, target));
                    }
                }
            }
        }
        for (caller, target) in edges {
            if let Some(callee) = definitions.get(&target) {
                graph.insert_edge(caller, *callee, EdgeKind::Calls)?;
            }
        }
        graph.set_attr(
            graph.root(),
            crate::ATTR_SEMANTIC_SNAPSHOT,
            serde_json::to_value(crate::SemanticSnapshot::new(self.revision, snapshots))?,
        )?;
        Ok(graph)
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
