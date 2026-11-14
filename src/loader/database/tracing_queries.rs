//! A file's tracing inventory as a query.
//!
//! The inventory is a function of the file's syntax, its attribute evidence
//! (a query already: `file_facts`) and the semantic facts of the functions it
//! declares (queries too: `function_evidence`). So it is keyed by the file alone
//! and reads the rest from the database: an edit recomputes the inventories of
//! the files whose functions' evidence changed, and no other.
//!
//! A function only a cfg context activates has no ordinary facts; its facts exist
//! only in the graph, after the context pass. A file holding one is read directly
//! by the projection, which has the graph.

use std::collections::HashMap;

use ra_ap_hir::{Semantics, attach_db};
use ra_ap_syntax::{AstNode, TextRange, ast};
use tracing::instrument;

use super::populate::expanded_definition;
use super::queries::{self, FileKey, SemanticDatabase};
use crate::ir::SourceTracingInventory;

/// One file's tracing inventory, or `None` when the file did not parse.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn tracing_inventory_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: FileKey<'db>,
) -> Option<SourceTracingInventory> {
    let (file, path) = (key.file(db), key.path(db));
    let root_db = db.root_database();
    attach_db(root_db, || {
        let sema = Semantics::new(root_db);
        let source = super::projection::source_of(root_db, &sema, file, path).ok()?;
        let facts = queries::file_facts(db, key).as_ref()?;
        tracing::debug!(target: "cordial::semantic::queries", function = %path.display(), query = "tracing_inventory_file", "semantic query executed");
        let mut semantics: HashMap<TextRange, crate::SemanticFunction> = HashMap::new();
        for function in source
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
        {
            let Some(definition) = sema
                .to_def(&function)
                .or_else(|| expanded_definition(&sema, &function))
            else {
                continue;
            };
            if let Some(found) = queries::evidence_of(db, definition).as_ref() {
                semantics.insert(function.syntax().text_range(), found.clone());
            }
        }
        SourceTracingInventory::collect(&source, facts.attributes(), &semantics).ok()
    })
}
