//! Calls that leave the crate being projected, located in source.
//!
//! A call whose callee has no node in this crate's graph is still evidence: the
//! callee may be a workspace function in another crate, whose own graph can
//! match it by location. The session id of the callee only means something
//! here, so it is turned into a source location now.

use std::path::PathBuf;

use ra_ap_hir::{HasSource, ModuleDef};
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::AstNode;

use super::types;
use crate::{FunctionLocation, SemanticItemId};

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
        let source = ra_ap_hir::attach_db(self.db, || function.source(self.db))?;
        if source.file_id.is_macro() {
            return None;
        }
        let file = source
            .file_id
            .original_file_respecting_includes(self.db)
            .file_id(self.db);
        let path = self.vfs.file_path(file).as_path()?;
        let path: &std::path::Path = path.as_ref();
        self.roots
            .iter()
            .any(|root| path.starts_with(root))
            .then(|| {
                FunctionLocation::new(
                    path.to_path_buf(),
                    u32::from(source.value.syntax().text_range().start()),
                )
            })
    }
}
