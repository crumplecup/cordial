//! The gates of a file, read once from its text and shared by everything that asks.
//!
//! The planning scan and each context's walk both need what a file's `cfg` attributes say: the
//! clauses that make each gated item active, and which names a gate defines or uses. That is a
//! function of the file's text alone, and a context's copy of a file has its original's text, so
//! it is read once from the original (not once more for every copy of it in every context). It is
//! a query of the database, so an edit to the file reads it again.

use super::flips::Flips;
use super::plan::{Clause, clauses, guard};
use crate::loader::database::edition::edition_of;
use crate::loader::database::item_key::{self, ItemKey};
use crate::loader::database::queries::SemanticDatabase;
use ra_ap_cfg::CfgExpr;
use ra_ap_hir::attach_db;
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{AstNode, SourceFile, TextRange, ast};
use ra_ap_vfs::FileId;
use std::collections::BTreeSet;
use tracing::instrument;

/// A `#[cfg(..)]` gate: the item it is on, the predicate composed with those of everything around
/// it, and the names written inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CfgHost {
    pub(super) range: TextRange,
    pub(super) predicate: CfgExpr,
    pub(super) names: BTreeSet<String>,
}

/// What a file's attributes say about its gates.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct FileGates {
    /// The clauses each attribute's item is active under, each list once.
    pub(super) clause_sets: BTreeSet<Vec<Clause>>,
    /// The items a `cfg` attribute is on.
    pub(super) cfg_hosts: Vec<CfgHost>,
}

impl FileGates {
    /// The gates `tree` has.
    #[instrument(level = "debug", skip_all)]
    #[hotpath::measure(impl_type = "FileGates")]
    fn of(tree: &SourceFile) -> Self {
        let mut gates = Self::default();
        for attr in tree.syntax().descendants().filter_map(ast::Attr::cast) {
            let Some(host) = attr.syntax().parent() else {
                continue;
            };
            let predicate = guard(&host);
            gates.clause_sets.insert(clauses(&predicate, false));
            if !matches!(attr.meta(), Some(ast::Meta::CfgMeta(_))) {
                continue;
            }
            let names = host
                .descendants()
                .filter_map(|node| {
                    ast::Name::cast(node.clone())
                        .map(|name| name.text().to_string())
                        .or_else(|| ast::NameRef::cast(node).map(|name| name.text().to_string()))
                })
                .collect();
            gates.cfg_hosts.push(CfgHost {
                range: host.text_range(),
                predicate,
                names,
            });
        }
        gates
    }
}

/// The gates of the file `scope` describes, read from its text.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, scope))]
#[hotpath::measure]
fn gates_in<'db>(db: &'db dyn SemanticDatabase, scope: ItemKey<'db>) -> FileGates {
    let root = db.root_database();
    attach_db(root, || {
        item_key::file_of_scope(root, &item_key::key_text(root, scope))
            .map_or_else(FileGates::default, |file| {
                FileGates::of(&edition_of(root, file).parse(root).tree())
            })
    })
}

/// The gates of `file`, which a context's copy shares with its original: ask for the original's.
#[instrument(level = "trace", skip(db))]
#[hotpath::measure]
pub(super) fn gates_of(db: &RootDatabase, file: FileId) -> Option<&FileGates> {
    let semantic: &dyn SemanticDatabase = db;
    Some(gates_in(semantic, item_key::file_scope(db, file)?))
}

impl Flips {
    /// The gates of `gates` that differ between any `(ordinary, context)` cfg pair: the same as
    /// reading them from the file's tree, from what was read once.
    #[instrument(level = "debug", skip_all)]
    #[hotpath::measure(impl_type = "Flips")]
    pub(super) fn of(
        gates: &FileGates,
        pairs: &[(ra_ap_cfg::CfgOptions, ra_ap_cfg::CfgOptions)],
    ) -> Self {
        Self::from_hosts(gates.cfg_hosts.iter().filter(|host| {
            pairs.iter().any(|(ordinary, context)| {
                super::flips::predicate_holds(ordinary, &host.predicate)
                    != super::flips::predicate_holds(context, &host.predicate)
            })
        }))
    }
}
