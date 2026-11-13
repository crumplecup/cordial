//! Calls that leave the crate being projected, located in source.
//!
//! A call whose callee has no node in this crate's graph is still evidence: the
//! callee may be a workspace function in another crate, whose own graph can
//! match it by location. The session id of the callee only means something
//! here, so it is turned into a source location now.

use std::path::PathBuf;

use ra_ap_hir::{AsAssocItem, AssocItem, AssocItemContainer, Function, HasSource, ModuleDef};
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{AstNode, ast};

use super::types;
use crate::{
    CordialResult, CrateIr, FunctionLocation, NodeId, SemanticItemId, TraitImplementation,
};

/// Finds where a function definition starts, for functions the workspace owns.
#[derive(derive_new::new)]
#[new(visibility = "pub(super)")]
pub(super) struct Locator<'a> {
    db: &'a RootDatabase,
    vfs: &'a ra_ap_vfs::Vfs,
    roots: Vec<PathBuf>,
}

impl Locator<'_> {
    /// The location of the function `id` names, or `None` for anything else: a
    /// non-function, a function from a dependency, or one a macro generated
    /// (its ranges are expansion coordinates, not source).
    #[tracing::instrument(level = "trace", skip(self, id))]
    pub(super) fn locate(&self, id: SemanticItemId) -> Option<FunctionLocation> {
        let ModuleDef::Function(function) = types::definition(self.db, id) else {
            return None;
        };
        self.location_of(function)
    }

    #[tracing::instrument(level = "trace", skip(self, function))]
    fn location_of(&self, function: Function) -> Option<FunctionLocation> {
        let (file_id, start) = ra_ap_hir::attach_db(self.db, || {
            let source = function.source(self.db)?;
            let mut file_id = source.file_id;
            let mut start = source.value.syntax().text_range().start();
            // A function an attribute macro generated is the annotated item as far
            // as callers are concerned: its location is where that item starts.
            while file_id.is_macro() {
                let call = file_id.call_node(self.db)?;
                if !ast::Fn::can_cast(call.value.kind()) {
                    return None;
                }
                start = call.value.text_range().start();
                file_id = call.file_id;
            }
            Some((file_id, start))
        })?;
        let file = file_id
            .original_file_respecting_includes(self.db)
            .file_id(self.db);
        let path = self.vfs.file_path(file).as_path()?;
        let path: &std::path::Path = path.as_ref();
        self.roots
            .iter()
            .any(|root| path.starts_with(root))
            .then(|| FunctionLocation::new(path.to_path_buf(), u32::from(start)))
    }

    /// Record on `node` the trait method the function `id` implements, if it
    /// implements one. The declaration's location is recorded when the trait is
    /// the workspace's; otherwise the fact says the declaration is foreign.
    #[tracing::instrument(level = "debug", skip(self, id, graph, node), err(level = "warn"))]
    #[hotpath::measure]
    pub(super) fn record_trait_implementation(
        &self,
        graph: &mut CrateIr,
        id: SemanticItemId,
        node: NodeId,
    ) -> CordialResult<()> {
        let ModuleDef::Function(function) = types::definition(self.db, id) else {
            return Ok(());
        };
        let declaration = ra_ap_hir::attach_db(self.db, || {
            let AssocItemContainer::Impl(implementation) =
                function.as_assoc_item(self.db)?.container(self.db)
            else {
                return None;
            };
            let name = function.name(self.db);
            implementation
                .trait_(self.db)?
                .items(self.db)
                .into_iter()
                .find_map(|item| match item {
                    AssocItem::Function(declared) if declared.name(self.db) == name => {
                        Some(declared)
                    }
                    _ => None,
                })
        });
        let Some(declared) = declaration else {
            return Ok(());
        };
        graph.set_attr(
            node,
            crate::ATTR_TRAIT_IMPLEMENTATION,
            serde_json::to_value(TraitImplementation::new(self.location_of(declared)))?,
        )
    }
}
