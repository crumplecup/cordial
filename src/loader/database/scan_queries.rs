//! The per-file half of each whole-crate collector, as Salsa queries.
//!
//! A collector walks the crate's files and merges one result per file. The
//! result for a file is a function of that file's text and parse (and of the
//! scan roots it is read under), so each is a tracked query keyed by the file:
//! a session computes it again only for a file whose text changed. The walk and
//! the merge stay where they were; they cost a copy, not an analysis.

use std::path::PathBuf;

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
use ra_ap_hir::Semantics;
#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
use ra_ap_ide_db::RootDatabase;
#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
use tracing::instrument;

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
use super::DatabaseSource;
#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
use super::queries::SemanticDatabase;

/// A file as one collector reads it: the database's id for it, the path the IR
/// names it by, the scan tree it was found under, and the crate root.
#[salsa::interned(persist)]
pub(super) struct ScanKey<'db> {
    #[returns(copy)]
    file: ra_ap_vfs::FileId,
    #[returns(ref)]
    path: PathBuf,
    #[returns(ref)]
    tree: PathBuf,
    #[returns(ref)]
    root: PathBuf,
    /// A name the collector reads the file under (the crate's), or empty.
    #[returns(ref)]
    name: String,
    /// A flag the collector reads the file under, or false.
    #[returns(copy)]
    flag: bool,
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
/// Build the file's carrier and run `collect` on it, or `None` when the file
/// does not parse or the collector refuses it. The caller falls back to the
/// uncached path in that case, which reports the error.
#[instrument(level = "debug", skip(db, collect), fields(query = query))]
#[hotpath::measure]
fn scanned<T>(
    db: &RootDatabase,
    key: (ra_ap_vfs::FileId, &std::path::Path),
    query: &str,
    collect: impl FnOnce(&DatabaseSource<'_>, &Semantics<'_, RootDatabase>) -> crate::CordialResult<T>,
) -> Option<T> {
    let (file, path) = key;
    let sema = Semantics::new(db);
    let source = super::projection::source_of(db, &sema, file, path).ok()?;
    tracing::debug!(target: "cordial::semantic::queries", function = %path.display(), query, "semantic query executed");
    collect(&source, &sema).ok()
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
/// A file as the fused extraction reads it: the database's id for it, the path the IR names it by,
/// and the crate root. The scan tree is the one of the root's that holds the path.
#[salsa::interned(persist)]
pub(super) struct ExtractKey<'db> {
    #[returns(copy)]
    file: ra_ap_vfs::FileId,
    #[returns(ref)]
    path: PathBuf,
    #[returns(ref)]
    root: PathBuf,
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
/// What every collector that reads only a file's tree finds in it, from one parse.
///
/// Each field is what the collector's own query gave for the file: `None` where it refused the
/// file, which the caller reads again outside the query to report why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Extracted {
    #[cfg(feature = "error_sites")]
    error_sites: Option<crate::ir::ErrorSitesFile>,
    #[cfg(feature = "error_chain")]
    error_chains: Option<crate::ir::ErrorChainsFile>,
    #[cfg(feature = "internal_error_chain")]
    error_types: Option<crate::ir::ErrorTypesFile>,
    #[cfg(feature = "internal_error_chain")]
    error_apply: Option<crate::ir::ErrorApplyFile>,
    #[cfg(feature = "tracing")]
    observability: Option<crate::ir::ObservabilityFile>,
    #[cfg(feature = "verus_ir")]
    verus: Option<crate::ir::VerusFile>,
    #[cfg(feature = "visibility")]
    visibility: Option<(crate::ir::VisibilityBody, crate::FileSnapshot)>,
    #[cfg(any(
        feature = "allows",
        feature = "cfg_hygiene",
        feature = "cfg_scatter",
        feature = "glob_imports",
        feature = "inline_tests",
        feature = "modularity",
        feature = "pageantry",
        feature = "derives",
        feature = "visibility",
        feature = "cli_layout"
    ))]
    policy: Option<PolicyFile>,
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
impl Extracted {
    /// Run every collector on one file's carrier.
    #[instrument(level = "debug", skip_all)]
    #[hotpath::measure(impl_type = "Extracted")]
    fn collect(
        source: &DatabaseSource<'_>,
        sema: &Semantics<'_, RootDatabase>,
        tree: &std::path::Path,
        root: &std::path::Path,
    ) -> Self {
        // Not every feature reads these.
        let _ = (sema, tree, root);
        Self {
            #[cfg(feature = "error_sites")]
            error_sites: crate::ir::SourceErrorSites::native_file(source, tree, root).ok(),
            #[cfg(feature = "error_chain")]
            error_chains: crate::ir::SourceErrorChains::native_file(source, tree, root).ok(),
            #[cfg(feature = "internal_error_chain")]
            error_types: crate::ir::SourceErrorTypes::native_file(source, tree).ok(),
            #[cfg(feature = "internal_error_chain")]
            error_apply: Some(crate::ir::SourceErrorApply::native_file(source)),
            #[cfg(feature = "tracing")]
            observability: Some(crate::ir::SourceObservability::native_file(
                source,
                tree,
                root,
                &|call| super::identity::macro_by_path(sema, call),
                &|path, scope| super::identity::macro_at(sema, scope, path),
            )),
            #[cfg(feature = "verus_ir")]
            verus: Some(crate::ir::VerusFile::new(
                crate::FileSnapshot::new(source.path().to_path_buf(), source.source_digest()),
                source.verus_facts(
                    &crate::loader::module_path_from_src_file(tree, source.path()).join("::"),
                    &|call| super::identity::is_verus_macro(sema, call),
                ),
            )),
            #[cfg(feature = "visibility")]
            visibility: {
                #[cfg(feature = "verus_ir")]
                {
                    crate::ir::SourcePolicyInventory::native_visibility_file(source, &|call| {
                        super::identity::is_verus_macro(sema, call)
                    })
                    .ok()
                }
                #[cfg(not(feature = "verus_ir"))]
                {
                    crate::ir::SourcePolicyInventory::native_visibility_file(source).ok()
                }
            },
            #[cfg(any(
                feature = "allows",
                feature = "cfg_hygiene",
                feature = "cfg_scatter",
                feature = "glob_imports",
                feature = "inline_tests",
                feature = "modularity",
                feature = "pageantry",
                feature = "derives",
                feature = "visibility",
                feature = "cli_layout"
            ))]
            policy: crate::ir::SourcePolicyInventory::native_file_part(source, root)
                .ok()
                .map(|part| PolicyFile {
                    part,
                    snapshot: crate::FileSnapshot::new(
                        source.path().to_path_buf(),
                        source.source_digest(),
                    ),
                }),
        }
    }
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
/// The scan tree of the crate at `root` that holds `path`: `src` or `tests`.
fn scan_tree(root: &std::path::Path, path: &std::path::Path) -> PathBuf {
    crate::loader::quality_scan_trees(root)
        .into_iter()
        .find(|tree| path.starts_with(tree))
        .unwrap_or_default()
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
/// Every collector's result for one file, from one parse: what the queries of the collectors read.
/// Not saved: what each collector's own query saves is what a later run reads.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
fn extract_file<'db>(db: &'db dyn SemanticDatabase, key: ExtractKey<'db>) -> Option<Extracted> {
    let root_db = db.root_database();
    let tree = scan_tree(key.root(db), key.path(db));
    scanned(
        root_db,
        (key.file(db), key.path(db)),
        "extract_file",
        |source, sema| {
            Ok(ra_ap_hir::attach_db(root_db, || {
                Extracted::collect(source, sema, &tree, key.root(db))
            }))
        },
    )
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
/// The fused extraction of the file a collector's key names.
fn extracted<'db>(db: &'db dyn SemanticDatabase, key: ScanKey<'db>) -> &'db Option<Extracted> {
    extract_file(
        db,
        ExtractKey::new(db, key.file(db), key.path(db).clone(), key.root(db).clone()),
    )
}

#[cfg(feature = "error_sites")]
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn error_sites_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::ErrorSitesFile> {
    extracted(db, key).as_ref()?.error_sites.clone()
}

#[cfg(feature = "error_chain")]
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn error_chains_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::ErrorChainsFile> {
    extracted(db, key).as_ref()?.error_chains.clone()
}

#[cfg(feature = "internal_error_chain")]
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn error_types_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::ErrorTypesFile> {
    extracted(db, key).as_ref()?.error_types.clone()
}

#[cfg(feature = "internal_error_chain")]
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn error_compliance_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::ErrorComplianceFile> {
    scanned(
        db.root_database(),
        (key.file(db), key.path(db)),
        "error_compliance_file",
        |source, _| {
            crate::ir::SourceErrorCompliance::native_file(
                source,
                key.tree(db),
                key.name(db),
                key.flag(db),
            )
        },
    )
}

#[cfg(feature = "tracing")]
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn observability_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::ObservabilityFile> {
    extracted(db, key).as_ref()?.observability.clone()
}

#[cfg(feature = "verus_ir")]
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn verus_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::VerusFile> {
    extracted(db, key).as_ref()?.verus.clone()
}

#[cfg(feature = "internal_error_chain")]
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn error_apply_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<crate::ir::ErrorApplyFile> {
    extracted(db, key).as_ref()?.error_apply.clone()
}

#[cfg(feature = "visibility")]
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn visibility_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<(crate::ir::VisibilityBody, crate::FileSnapshot)> {
    extracted(db, key).as_ref()?.visibility.clone()
}

/// One policy file's contribution to the crate's inventory, and its snapshot.
#[cfg(any(
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "visibility",
    feature = "cli_layout"
))]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub(super) struct PolicyFile {
    part: crate::ir::SourcePolicyInventory,
    snapshot: crate::FileSnapshot,
}

#[cfg(any(
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "visibility",
    feature = "cli_layout"
))]
impl PolicyFile {
    /// What the file adds to the inventory.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn part(&self) -> &crate::ir::SourcePolicyInventory {
        &self.part
    }

    /// The file's path and digest.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn snapshot(&self) -> &crate::FileSnapshot {
        &self.snapshot
    }
}

#[cfg(any(
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "visibility",
    feature = "cli_layout"
))]
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn policy_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: ScanKey<'db>,
) -> Option<PolicyFile> {
    extracted(db, key).as_ref()?.policy.clone()
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
/// Compute what every collector finds in one file, if it is not known: for the pass that does it
/// before the steps ask.
pub(super) fn prime_extraction(
    db: &RootDatabase,
    file: ra_ap_vfs::FileId,
    path: &std::path::Path,
    root: &std::path::Path,
) {
    // The errors of the parse, which the steps check for each file, so that they need not hold the
    // tree for it.
    let _ = super::edition::edition_of(db, file).parse_errors(db);
    // Through what each collector saves, and not the extraction, which is not saved: what a
    // restored run has is valid, and nothing is computed; where it is not, the first to ask computes
    // the extraction and the others read it.
    let key = ScanKey::new(
        db,
        file,
        path.to_path_buf(),
        PathBuf::new(),
        root.to_path_buf(),
        String::new(),
        false,
    );
    #[cfg(feature = "error_sites")]
    let _ = error_sites_file(db, key);
    #[cfg(feature = "error_chain")]
    let _ = error_chains_file(db, key);
    #[cfg(feature = "internal_error_chain")]
    {
        let _ = error_types_file(db, key);
        let _ = error_apply_file(db, key);
    }
    #[cfg(feature = "tracing")]
    let _ = observability_file(db, key);
    #[cfg(feature = "verus_ir")]
    let _ = verus_file(db, key);
    #[cfg(feature = "visibility")]
    let _ = visibility_file(db, key);
    #[cfg(any(
        feature = "allows",
        feature = "cfg_hygiene",
        feature = "cfg_scatter",
        feature = "glob_imports",
        feature = "inline_tests",
        feature = "modularity",
        feature = "pageantry",
        feature = "derives",
        feature = "visibility",
        feature = "cli_layout"
    ))]
    let _ = policy_file(db, key);
}

#[cfg(feature = "internal_error_chain")]
/// Compute what the compliance collector finds in one file, if it is not known. Its key has the crate's
/// name and whether it is a proc-macro crate, which the other collectors' keys do not.
pub(super) fn prime_compliance(
    db: &RootDatabase,
    file: ra_ap_vfs::FileId,
    path: &std::path::Path,
    root: &std::path::Path,
    name: &str,
    proc_macro: bool,
) {
    let key = ScanKey::new(
        db,
        file,
        path.to_path_buf(),
        scan_tree(root, path),
        root.to_path_buf(),
        name.to_owned(),
        proc_macro,
    );
    let _ = error_compliance_file(db, key);
}

/// Who owns what a file's queries computed: the cfg context whose copy the file is, or nobody
/// when it is the ordinary build's.
///
/// A context's copies are all in one source root of their own, whose crates are the context's
/// copied crates; those are the roots that decide when the context's values may be retired. A copy
/// root with no crate recorded cannot be told from a context's, so its values are preserved.
/// Nothing is analysed: this reads two inputs.
#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
#[tracing::instrument(level = "trace", skip(db))]
#[hotpath::measure]
fn file_ownership(
    db: &ra_ap_ide_db::RootDatabase,
    file: ra_ap_vfs::FileId,
) -> salsa::QueryOwnership {
    use ra_ap_ide_db::base_db::{SourceDatabase as _, TOOL_SOURCE_ROOTS};
    use salsa::plumbing::AsId;
    let root = db.file_source_root(file).source_root_id(db);
    if root.0 < TOOL_SOURCE_ROOTS {
        return salsa::QueryOwnership::Shared;
    }
    let crates = db.source_root(root).crates(db);
    if crates.is_empty() {
        return salsa::QueryOwnership::Unclassified;
    }
    salsa::QueryOwnership::owned(
        crates
            .iter()
            .map(AsId::as_id)
            .filter(|&id| salsa::is_owner_root(db, id)),
    )
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
#[tracing::instrument(level = "trace", skip(db))]
#[hotpath::measure]
fn extract_owners(db: &dyn SemanticDatabase, id: salsa::Id) -> salsa::QueryOwnership {
    use salsa::plumbing::FromId;
    file_ownership(db.root_database(), ExtractKey::from_id(id).file(db))
}

#[cfg(any(
    feature = "error_sites",
    feature = "error_chain",
    feature = "internal_error_chain",
    feature = "tracing",
    feature = "verus_ir",
    feature = "visibility",
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "cli_layout"
))]
#[tracing::instrument(level = "trace", skip(db))]
#[hotpath::measure]
fn scan_owners(db: &dyn SemanticDatabase, id: salsa::Id) -> salsa::QueryOwnership {
    use salsa::plumbing::FromId;
    file_ownership(db.root_database(), ScanKey::from_id(id).file(db))
}

/// Install the ownership of the per-file queries, so that a context's copied files release what
/// was computed from them when the context finishes. Once after a database is built or restored,
/// before these queries run; a registration is a cache policy and changes no semantic input.
#[tracing::instrument(level = "debug", skip(db))]
pub(super) fn register_query_ownership(db: &mut impl salsa::Database) {
    #[cfg(any(
        feature = "error_sites",
        feature = "error_chain",
        feature = "internal_error_chain",
        feature = "tracing",
        feature = "verus_ir",
        feature = "visibility",
        feature = "allows",
        feature = "cfg_hygiene",
        feature = "cfg_scatter",
        feature = "glob_imports",
        feature = "inline_tests",
        feature = "modularity",
        feature = "pageantry",
        feature = "derives",
        feature = "cli_layout"
    ))]
    salsa::register_query_ownership::<extract_file, dyn SemanticDatabase>(db, extract_owners);
    #[cfg(feature = "error_sites")]
    salsa::register_query_ownership::<error_sites_file, dyn SemanticDatabase>(db, scan_owners);
    #[cfg(feature = "error_chain")]
    salsa::register_query_ownership::<error_chains_file, dyn SemanticDatabase>(db, scan_owners);
    #[cfg(feature = "internal_error_chain")]
    {
        salsa::register_query_ownership::<error_types_file, dyn SemanticDatabase>(db, scan_owners);
        salsa::register_query_ownership::<error_compliance_file, dyn SemanticDatabase>(
            db,
            scan_owners,
        );
        salsa::register_query_ownership::<error_apply_file, dyn SemanticDatabase>(db, scan_owners);
    }
    #[cfg(feature = "tracing")]
    salsa::register_query_ownership::<observability_file, dyn SemanticDatabase>(db, scan_owners);
    #[cfg(feature = "verus_ir")]
    salsa::register_query_ownership::<verus_file, dyn SemanticDatabase>(db, scan_owners);
    #[cfg(feature = "visibility")]
    salsa::register_query_ownership::<visibility_file, dyn SemanticDatabase>(db, scan_owners);
    #[cfg(any(
        feature = "allows",
        feature = "cfg_hygiene",
        feature = "cfg_scatter",
        feature = "glob_imports",
        feature = "inline_tests",
        feature = "modularity",
        feature = "pageantry",
        feature = "derives",
        feature = "visibility",
        feature = "cli_layout"
    ))]
    salsa::register_query_ownership::<policy_file, dyn SemanticDatabase>(db, scan_owners);
    let _ = db;
}

/// A database with nothing loaded, with the ownership of the per-file queries installed.
#[tracing::instrument(level = "debug")]
pub(super) fn empty_database() -> ra_ap_ide_db::RootDatabase {
    let mut db = ra_ap_ide_db::RootDatabase::new(None);
    register_query_ownership(&mut db);
    db
}
