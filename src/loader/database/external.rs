//! Calls that leave the crate being projected, located in source.
//!
//! A call whose callee has no node in this crate's graph is still evidence: the
//! callee may be a workspace function in another crate, whose own graph can
//! match it by location. The session id of the callee only means something
//! here, so it is turned into a source location now.

use std::path::PathBuf;

use ra_ap_hir::{AsAssocItem, AssocItem, AssocItemContainer, Function, HasSource};
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{AstNode, ast};

use super::item_key::{self, ItemKey};
use super::queries::SemanticDatabase;
use crate::{
    CordialResult, CrateIr, FunctionLocation, NodeId, SemanticItemId, TraitImplementation,
};

/// Finds where a function definition starts, for functions the workspace owns.
#[derive(derive_new::new)]
#[new(visibility = "pub(super)")]
pub(super) struct Locator<'a> {
    db: &'a RootDatabase,
    roots: Vec<PathBuf>,
}

impl Locator<'_> {
    /// The location of the function `id` names, or `None` for anything else: a
    /// non-function, a function from a dependency, or one a macro generated
    /// (its ranges are expansion coordinates, not source).
    #[tracing::instrument(level = "trace", skip(self, id))]
    pub(super) fn locate(&self, id: SemanticItemId) -> Option<FunctionLocation> {
        let db: &dyn SemanticDatabase = self.db;
        self.within_roots(start_of(db, item_key::key_for(id)).as_ref()?)
    }

    /// The location, if the workspace owns the file.
    #[tracing::instrument(level = "trace", skip(self))]
    fn within_roots(&self, start: &(PathBuf, u32)) -> Option<FunctionLocation> {
        let (path, start) = start;
        self.roots
            .iter()
            .any(|root| path.starts_with(root))
            .then(|| FunctionLocation::new(path.clone(), *start))
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
        let db: &dyn SemanticDatabase = self.db;
        let Some(declared) = implemented_declaration(db, item_key::key_for(id)) else {
            return Ok(());
        };
        let declared = &declared.location;
        graph.set_attr(
            node,
            crate::ATTR_TRAIT_IMPLEMENTATION,
            serde_json::to_value(TraitImplementation::new(
                declared.as_ref().and_then(|start| self.within_roots(start)),
            ))?,
        )
    }
}

/// Where the function `key` describes starts: its file, as the original's path for a context's
/// copy, and the offset; the annotated item's for a function an attribute macro generated. `None`
/// for anything else. Saved with the database: it is the same until the sources change, and a
/// restored run need not ask rust-analyzer.
#[salsa::tracked(persist, returns(ref))]
#[tracing::instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
fn start_of<'db>(db: &'db dyn SemanticDatabase, key: ItemKey<'db>) -> Option<(PathBuf, u32)> {
    let root = db.root_database();
    let function = item_key::function_of(db, key)?;
    start_of_function(root, function)
}

/// The trait method a function implements, and where the trait declares it. A struct, and not a
/// nested `Option`, because a saved `Some(None)` reads back as `None`: a method of a trait whose
/// declaration cannot be located would read back as no trait method at all.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Declared {
    location: Option<(PathBuf, u32)>,
}

/// Whether the function `key` describes implements a trait method, and if it does, where the
/// trait declares that method: see [`start_of`]. The location is `None` for a trait that cannot be
/// located.
#[salsa::tracked(persist, returns(ref))]
#[tracing::instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
fn implemented_declaration<'db>(
    db: &'db dyn SemanticDatabase,
    key: ItemKey<'db>,
) -> Option<Declared> {
    let root = db.root_database();
    let function = item_key::function_of(db, key)?;
    let declared = ra_ap_hir::attach_db(root, || {
        let AssocItemContainer::Impl(implementation) =
            function.as_assoc_item(root)?.container(root)
        else {
            return None;
        };
        let name = function.name(root);
        implementation
            .trait_(root)?
            .items(root)
            .into_iter()
            .find_map(|item| match item {
                AssocItem::Function(declared) if declared.name(root) == name => Some(declared),
                _ => None,
            })
    })?;
    Some(Declared {
        location: start_of_function(root, declared),
    })
}

#[tracing::instrument(level = "trace", skip(db, function))]
fn start_of_function(db: &RootDatabase, function: Function) -> Option<(PathBuf, u32)> {
    let (file_id, start) = ra_ap_hir::attach_db(db, || {
        let source = function.source(db)?;
        let mut file_id = source.file_id;
        let mut start = source.value.syntax().text_range().start();
        // A function an attribute macro generated is the annotated item as far
        // as callers are concerned: its location is where that item starts.
        while file_id.is_macro() {
            let call = file_id.call_node(db)?;
            if !ast::Fn::can_cast(call.value.kind()) {
                return None;
            }
            start = call.value.text_range().start();
            file_id = call.file_id;
        }
        Some((file_id, start))
    })?;
    let file = file_id.original_file_respecting_includes(db).file_id(db);
    let vfs_path = item_key::vfs_path_of(db, file)?;
    // A function in a context's copy of the workspace is located where the
    // original is: the copy has the same text, so the offset is the same.
    let path: PathBuf = match vfs_path.as_path() {
        Some(real) => AsRef::<std::path::Path>::as_ref(real).to_path_buf(),
        None => super::context_crates::original_path(&vfs_path)?,
    };
    Some((path, u32::from(start)))
}
