//! Collect crate-wide tracing evidence from canonical database trees.
use super::SourceDatabase;
use crate::loader::database::scan_queries::{self, ScanKey};
use crate::{CordialResult, ir::SourceObservability, loader::DatabaseSource};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use std::path::Path;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(in crate::loader::database) fn project_observability(
        &self,
        root: &Path,
        context: Option<&crate::ContextCrates>,
    ) -> CordialResult<SourceObservability> {
        let held = self.held_files(root)?;
        SourceObservability::collect_with(root, &held, |path, tree| {
            let original = self.file_id(path)?;
            let file = context
                .and_then(|context| context.copy_of(original))
                .unwrap_or(original);
            let key = ScanKey::new(
                &self.db,
                file,
                path.to_path_buf(),
                tree.to_path_buf(),
                root.to_path_buf(),
                String::new(),
                false,
            );
            match scan_queries::observability_file(&self.db, key) {
                Some(found) => Ok(found.clone()),
                None => self.observability_direct(file, path, tree, root),
            }
        })
    }
}

impl SourceDatabase {
    /// One file's observability facts, computed outside the query: for a file
    /// the query declined, which also reports why (or records a fixture that
    /// does not parse).
    #[tracing::instrument(level = "debug", skip(self, tree, root), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn observability_direct(
        &self,
        file: ra_ap_vfs::FileId,
        path: &Path,
        tree: &Path,
        root: &Path,
    ) -> CordialResult<crate::ir::ObservabilityFile> {
        let sema = Semantics::new(&self.db);
        let edition = super::super::edition::edition_of(&self.db, file);
        if let Some(error) = edition.parse(&self.db).errors().first() {
            if crate::loader::path_has_fixtures(path, root) {
                return Ok(SourceObservability::invalid_file(
                    path,
                    root,
                    &self.db.file_text(file).text(&self.db),
                ));
            }
            return Err(anyhow::Error::new(error.clone()).into());
        }
        let source = DatabaseSource::new(
            &self.db,
            path,
            self.db.file_text(file).text(&self.db),
            sema.parse(edition),
            ra_ap_ide_db::line_index(&self.db, file),
        );
        Ok(ra_ap_hir::attach_db(&self.db, || {
            SourceObservability::native_file(
                &source,
                tree,
                root,
                &|call| super::super::identity::macro_by_path(&sema, call),
                &|path, scope| super::super::identity::macro_at(&sema, scope, path),
            )
        }))
    }
}

impl SourceDatabase {
    /// Give each function's observability facts the callees of its semantic facts,
    /// from the ordinary build and from every cfg context that resolved it, so code
    /// only a verifier's or a feature's build activates is read too.
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_observability_calls(
        &self,
        outline: &super::declarations::Outline,
        graph: &mut crate::CrateIr,
    ) -> CordialResult<()> {
        use crate::IrView;
        let mut inventory = graph.source_observability()?;
        let mut failure = None;
        let leaves = graph.leaf_functions()?;
        inventory.apply_calls(|path, range| {
            let Some(node) = outline.functions().get(
                path,
                ra_ap_syntax::TextRange::new(
                    (range.start as u32).into(),
                    (range.end as u32).into(),
                ),
            ) else {
                // A leaf crate's function, which the graph holds no node for.
                let at = crate::FunctionLocation::new(path.to_path_buf(), range.start as u32);
                return leaves
                    .iter()
                    .find(|leaf| leaf.location() == &at)
                    .map_or_else(crate::ir::ResolvedCalls::default, |leaf| {
                        crate::ir::ResolvedCalls::new(
                            leaf.callees().clone(),
                            leaf.called().clone(),
                            false,
                        )
                    });
            };
            // The ordinary facts were typed when the file's items were built; only
            // a cfg context's are read back from the graph.
            let ordinary = outline.observed().get(&node);
            let mut callees: Vec<crate::ir::SemanticCallee> = ordinary
                .map(|observed| observed.callees().clone())
                .unwrap_or_default();
            let mut fallible = ordinary.is_some_and(|observed| *observed.fallible());
            let mut keys = Vec::new();
            match graph.semantic_context_functions(node) {
                Ok(contexts) => {
                    for entry in contexts {
                        keys.push(entry.context().clone());
                        let function = entry.function();
                        callees.extend(
                            function
                                .calls()
                                .iter()
                                .filter_map(|call| call.callee().clone()),
                        );
                        fallible |= matches!(
                            function.result_recording(),
                            crate::SemanticResultRecording::Result { .. }
                        );
                    }
                }
                Err(error) => failure = Some(error),
            }
            let mut targets = graph.children(node, crate::EdgeKind::Calls);
            for key in keys {
                targets.extend(graph.context_children(node, &key));
            }
            let mut called = Vec::new();
            for target in targets {
                match graph.function_location(target) {
                    Ok(location) => called.extend(location),
                    Err(error) => failure = Some(error),
                }
            }
            crate::ir::ResolvedCalls::new(callees, called, fallible)
        });
        if let Some(error) = failure {
            return Err(error);
        }
        graph.set_attr(
            graph.root(),
            crate::ir::ATTR_SOURCE_OBSERVABILITY,
            serde_json::to_value(inventory)?,
        )
    }
}
