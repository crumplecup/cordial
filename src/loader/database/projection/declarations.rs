//! Source declaration nodes, semantic edges and their exact input identities.

use super::SourceDatabase;
use crate::loader::database::{DatabaseSource, contexts::FunctionNodes, populate::Projection};
#[cfg(feature = "tracing")]
use crate::loader::database::{queries, tracing_queries};
use crate::{CordialResult, CrateIr, FileSnapshot};
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
    modules: Vec<(PathBuf, crate::NodeId)>,
}

impl SourceDatabase {
    /// The database-issued carrier for `path`, refusing a file that did not parse.
    #[tracing::instrument(level = "debug", skip(self, sema, path), err(level = "warn"))]
    #[hotpath::measure]
    pub(in crate::loader::database) fn declaration_source<'a>(
        &'a self,
        sema: &Semantics<'a, RootDatabase>,
        path: &'a Path,
    ) -> CordialResult<(ra_ap_vfs::FileId, DatabaseSource<'a>)> {
        let file = self.file_id(path)?;
        Ok((file, source_of(&self.db, sema, file, path)?))
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
            let (file, _source) = self.declaration_source(&sema, path)?;
            snapshots.push(FileSnapshot::new(
                path.canonicalize()?,
                Sha256::digest(self.db.file_text(file).text(&self.db).as_bytes()).into(),
            ));
            let module = projection.populate(&self.db, file, path, &src, graph)?;
            #[cfg(feature = "tracing")]
            modules.push((path.clone(), module));
            #[cfg(not(feature = "tracing"))]
            let _ = module;
        }
        let roots = self
            .projection_context()?
            .packages()
            .iter()
            .map(|package| package.root().clone())
            .collect();
        let locator = super::super::external::Locator::new(&self.db, &self.vfs, roots);
        let functions = projection.connect(graph, &locator)?;
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
                let file = self.file_id(path)?;
                let key = queries::FileKey::new(&self.db, file, path.clone());
                let (_, source) = self.declaration_source(&sema, path)?;
                let Some(facts) = queries::file_facts(&self.db, key) else {
                    return Err(crate::CordialError::workspace_source_facts_missing(
                        "file facts",
                    ));
                };
                let attributes = facts.attributes();
                let ranges: Vec<_> = attributes
                    .items()
                    .iter()
                    .filter_map(|item| {
                        let range = ra_ap_syntax::TextRange::new(
                            (item.range().start() as u32).into(),
                            (item.range().end() as u32).into(),
                        );
                        Some((range, outline.functions().get(path, range)?))
                    })
                    .collect();
                // A function only a cfg context resolves has its facts in the
                // graph, not the database: noticed without decoding any facts.
                let mut context_only = false;
                for (_, node) in &ranges {
                    if !crate::IrView::node(graph, *node)
                        .is_some_and(|found| found.attr(crate::ATTR_SEMANTIC_FUNCTION).is_some())
                        && !crate::IrView::semantic_context_functions(graph, *node)?.is_empty()
                    {
                        context_only = true;
                        break;
                    }
                }
                let inventory = match tracing_queries::tracing_inventory_file(&self.db, key) {
                    Some(found) if !context_only => found.clone(),
                    // A context-only function, or a file the query declined: read
                    // it with the facts the graph holds, which reports why.
                    _ => {
                        let mut semantics = std::collections::HashMap::new();
                        for (range, node) in &ranges {
                            let facts = match crate::IrView::semantic_function(graph, *node)? {
                                Some(facts) => Some(facts),
                                None => crate::IrView::semantic_context_functions(graph, *node)?
                                    .first()
                                    .map(|entry| entry.function().clone()),
                            };
                            if let Some(facts) = facts {
                                semantics.insert(*range, facts);
                            }
                        }
                        crate::ir::SourceTracingInventory::collect(&source, attributes, &semantics)?
                    }
                };
                graph.set_attr(
                    *module,
                    crate::ir::ATTR_SOURCE_TRACING_INVENTORY,
                    serde_json::to_value(inventory)?,
                )?;
            }
            Ok(())
        })
    }
}

/// The carrier of `file`'s text, parse and line index, refusing a file that did
/// not parse. The same one serves the projection and the per-file queries, so
/// both read the file as the database holds it.
#[tracing::instrument(level = "debug", skip(db, sema, path), err(level = "warn"))]
#[hotpath::measure]
pub(in crate::loader::database) fn source_of<'a>(
    db: &'a RootDatabase,
    sema: &Semantics<'a, RootDatabase>,
    file: ra_ap_vfs::FileId,
    path: &'a Path,
) -> CordialResult<DatabaseSource<'a>> {
    let text = db.file_text(file).text(db);
    let edition = sema.attach_first_edition(file);
    // Semantics consults this same memoized parse. Recovery cannot
    // authorize a complete declaration or edit inventory.
    if let Some(error) = edition.parse(db).errors().first() {
        return Err(anyhow::Error::new(error.clone()).into());
    }
    Ok(DatabaseSource::new(
        #[cfg(any(
            feature = "tracing",
            feature = "internal_error_chain",
            feature = "verus_ir",
            feature = "panics",
            feature = "antipatterns"
        ))]
        db,
        path,
        text,
        sema.parse(edition),
        ra_ap_ide_db::line_index(db, file),
    ))
}
