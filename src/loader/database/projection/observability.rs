//! Collect crate-wide tracing evidence from canonical database trees.
use super::SourceDatabase;
use crate::{CordialResult, ir::SourceObservability, loader::DatabaseSource};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use std::path::Path;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_observability(&self, root: &Path) -> CordialResult<SourceObservability> {
        let sema = Semantics::new(&self.db);
        SourceObservability::collect_with(root, |path, tree| {
            let file = self.file_id(path)?;
            let edition = sema.attach_first_edition(file);
            if let Some(error) = edition.parse(&self.db).errors().first() {
                if crate::loader::path_has_fixtures(path, root) {
                    return Ok(SourceObservability::invalid_file(
                        path,
                        root,
                        self.db.file_text(file).text(&self.db),
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
        })
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
                        crate::ir::ResolvedCalls::new(leaf.callees().clone(), leaf.called().clone())
                    });
            };
            let mut facts = Vec::new();
            match graph.semantic_function(node) {
                Ok(ordinary) => facts.extend(ordinary),
                Err(error) => failure = Some(error),
            }
            let mut keys = Vec::new();
            match graph.semantic_context_functions(node) {
                Ok(contexts) => {
                    for entry in contexts {
                        keys.push(entry.context().clone());
                        facts.push(entry.function().clone());
                    }
                }
                Err(error) => failure = Some(error),
            }
            let callees = facts
                .iter()
                .flat_map(|function| function.calls())
                .filter_map(|call| call.callee().clone())
                .collect();
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
            crate::ir::ResolvedCalls::new(callees, called)
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
