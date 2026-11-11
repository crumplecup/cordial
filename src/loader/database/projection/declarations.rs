//! Source declaration nodes, semantic edges and their exact input identities.

use super::SourceDatabase;
use crate::loader::database::{DatabaseSource, contexts::FunctionNodes, populate::Projection};
use crate::loader::module_path_from_src_file;
use crate::{CordialResult, CrateIr, FileSnapshot, NodeId};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::{RootDatabase, base_db::SourceDatabase as RaSourceDatabase};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Where the ordinary pass put each file's module and each function item, for
/// the passes that add cfg-context facts and the tracing inventory afterwards.
#[derive(Debug, Default, derive_getters::Getters)]
pub(in crate::loader::database) struct Outline {
    functions: FunctionNodes,
    #[cfg(feature = "tracing")]
    modules: Vec<(PathBuf, NodeId)>,
}

impl SourceDatabase {
    /// The database-issued carrier for `path`, refusing a file that did not parse.
    #[tracing::instrument(level = "debug", skip(self, sema, path), err(level = "warn"))]
    pub(in crate::loader::database) fn declaration_source<'a>(
        &'a self,
        sema: &Semantics<'a, RootDatabase>,
        path: &'a Path,
    ) -> CordialResult<(ra_ap_vfs::FileId, DatabaseSource<'a>)> {
        let file = self.file_id(path)?;
        let text = self.db.file_text(file).text(&self.db);
        let edition = sema.attach_first_edition(file);
        // Semantics consults this same memoized parse. Recovery cannot
        // authorize a complete declaration or edit inventory.
        if let Some(error) = edition.parse(&self.db).errors().first() {
            return Err(anyhow::Error::new(error.clone()).into());
        }
        let source = DatabaseSource::new(
            #[cfg(any(
                feature = "tracing",
                feature = "internal_error_chain",
                feature = "verus_ir",
                feature = "panics",
                feature = "antipatterns"
            ))]
            &self.db,
            path,
            text,
            sema.parse(edition),
            ra_ap_ide_db::line_index(&self.db, file),
        );
        Ok((file, source))
    }

    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_declarations(
        &self,
        crate_root: &Path,
        paths: &[PathBuf],
        graph: &mut CrateIr,
    ) -> CordialResult<(Vec<FileSnapshot>, Outline)> {
        let src = crate_root.join("src");
        let mut snapshots = Vec::new();
        #[cfg(feature = "tracing")]
        let mut modules = Vec::new();
        let mut projection = Projection::default();
        let sema = Semantics::new(&self.db);
        for path in paths {
            let (file, source) = self.declaration_source(&sema, path)?;
            snapshots.push(FileSnapshot::new(
                path.canonicalize()?,
                Sha256::digest(self.db.file_text(file).text(&self.db).as_bytes()).into(),
            ));
            let module_path = sema
                .file_to_module_defs(file)
                .next()
                .map(|module| {
                    module
                        .path_to_root(&self.db)
                        .into_iter()
                        .rev()
                        .filter_map(|module| module.name(&self.db))
                        .map(|name| name.as_str().to_owned())
                        .collect::<Vec<_>>()
                        .join("::")
                })
                // A file outside the module tree (a stray or unreferenced source)
                // still gets the path its location implies, as the policy scans
                // always gave it.
                .unwrap_or_else(|| module_path_from_src_file(&src, path).join("::"));
            let module = projection.populate(&self.db, &sema, &source, &module_path, graph)?;
            #[cfg(feature = "tracing")]
            modules.push((path.clone(), module));
            #[cfg(not(feature = "tracing"))]
            let _ = module;
        }
        let functions = projection.connect(graph)?;
        let outline = Outline {
            functions,
            #[cfg(feature = "tracing")]
            modules,
        };
        Ok((snapshots, outline))
    }

    /// Assemble each file's tracing inventory from the facts now in `graph`: a
    /// function's ordinary facts, or those of the first context that resolved it.
    #[cfg(feature = "tracing")]
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_tracing_inventories(
        &self,
        outline: &Outline,
        graph: &mut CrateIr,
    ) -> CordialResult<()> {
        ra_ap_hir::attach_db(&self.db, || {
            let sema = Semantics::new(&self.db);
            for (path, module) in outline.modules() {
                let (_, source) = self.declaration_source(&sema, path)?;
                let attributes = crate::ir::SourceFileAttributes::collect(&source);
                let mut semantics = std::collections::HashMap::new();
                for item in attributes.items() {
                    let range = ra_ap_syntax::TextRange::new(
                        (item.range().start() as u32).into(),
                        (item.range().end() as u32).into(),
                    );
                    let Some(node) = outline.functions().get(path, range) else {
                        continue;
                    };
                    let facts = match crate::IrView::semantic_function(graph, node)? {
                        Some(facts) => Some(facts),
                        None => crate::IrView::semantic_context_functions(graph, node)?
                            .first()
                            .map(|entry| entry.function().clone()),
                    };
                    if let Some(facts) = facts {
                        semantics.insert(range, facts);
                    }
                }
                graph.set_attr(
                    *module,
                    crate::ir::ATTR_SOURCE_TRACING_INVENTORY,
                    serde_json::to_value(crate::ir::SourceTracingInventory::collect(
                        &source,
                        &attributes,
                        &semantics,
                    )?)?,
                )?;
            }
            Ok(())
        })
    }
}
