//! The functions of a context's copy of a file, saved with the database.
//!
//! Naming what a context's copy of a file declares asks rust-analyzer for each function's
//! definition, which builds the definition maps of the copy's crate and of everything under it. The
//! answer depends only on what the sources and the crate graph say, so it is a saved query: a run
//! that changed nothing the copy's definitions read takes it from the saved state and builds no map.

use super::super::item_key::{self, ItemKey};
use super::super::populate::expanded_definition;
use super::super::queries::SemanticDatabase;
use ra_ap_hir::{ModuleDef, Semantics, attach_db};
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{AstNode, ast};
use ra_ap_vfs::FileId;
use tracing::instrument;

/// One function a context's copy of a file declares.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, derive_getters::Getters,
)]
pub(super) struct CopyFunction {
    /// Where the function's item starts and ends in the file.
    start: u32,
    end: u32,
    /// The function's own description: the copy's.
    key: String,
    /// Whether the description finds the function again, so that its evidence is the saved
    /// evidence of the description.
    found: bool,
    /// Where the function starts, which its saved evidence's ranges are relative to; `None` for a
    /// function a macro generated, whose ranges are not the file's.
    anchor: Option<u32>,
}

/// The functions of the file `scope` describes, with a definition under the file's cfg.
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, scope))]
#[hotpath::measure]
fn functions_in_copy<'db>(
    db: &'db dyn SemanticDatabase,
    scope: ItemKey<'db>,
) -> Option<Vec<CopyFunction>> {
    let root = db.root_database();
    attach_db(root, || {
        let file = item_key::file_of_scope(root, &item_key::key_text(root, scope))?;
        let sema = Semantics::new(root);
        let tree = sema.parse(sema.attach_first_edition_opt(file)?);
        let mut found = Vec::new();
        for function in tree.syntax().descendants().filter_map(ast::Fn::cast) {
            let Some(definition) = sema
                .to_def(&function)
                .or_else(|| expanded_definition(&sema, &function))
            else {
                continue;
            };
            let range = function.syntax().text_range();
            let key = item_key::key_of(root, ModuleDef::Function(definition));
            let anchor = sema
                .source(definition)
                .filter(|source| !source.file_id.is_macro())
                .map(|source| u32::from(source.value.syntax().text_range().start()));
            found.push(CopyFunction {
                start: u32::from(range.start()),
                end: u32::from(range.end()),
                key: item_key::key_text(root, key),
                found: item_key::finds(db, key, ModuleDef::Function(definition)),
                anchor,
            });
        }
        Some(found)
    })
}

/// The functions of `file`, a context's copy, from the saved list; `None` when it cannot be made.
#[instrument(level = "trace", skip(db))]
#[hotpath::measure]
pub(super) fn copy_functions(db: &RootDatabase, file: FileId) -> Option<&Vec<CopyFunction>> {
    let semantic: &dyn SemanticDatabase = db;
    functions_in_copy(semantic, item_key::file_scope(db, file)?).as_ref()
}
