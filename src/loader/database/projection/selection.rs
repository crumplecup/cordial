//! Package source selection and Cargo identities for production projection.

use super::SourceDatabase;
use crate::{CordialResult, CrateIr, CrateTarget};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

impl SourceDatabase {
    /// Collect a package without a caller-maintained file list. Inactive and
    /// unreferenced source files remain policy inputs. Integration tests supply
    /// policy evidence without becoming library call-graph nodes.
    #[tracing::instrument(level = "debug", skip(self, target), err(level = "warn"))]
    #[hotpath::measure]
    pub fn project_target(&mut self, target: &CrateTarget) -> CordialResult<CrateIr> {
        let root = target.crate_root();
        let context = self.projection_context()?;
        let root = root.canonicalize()?;
        let package = context
            .packages()
            .iter()
            .chain(&self.external_packages)
            .find(|package| package.root() == &root)
            .ok_or_else(|| crate::CordialError::not_found(root.clone()))?;
        let graph = self.package_sources(package);
        let mut policies = BTreeSet::new();
        for tree in crate::loader::quality_scan_trees(&root) {
            let is_source = tree == root.join("src");
            if !is_source
                && !cfg!(any(
                    feature = "allows",
                    feature = "glob_imports",
                    feature = "pageantry"
                ))
            {
                continue;
            }
            for path in rust_files(&tree) {
                if (is_source
                    && cfg!(any(
                        feature = "cfg_hygiene",
                        feature = "cfg_scatter",
                        feature = "modularity",
                        feature = "derives"
                    )))
                    || !is_fixture(&path, &root)
                {
                    policies.insert(path);
                }
            }
        }
        self.project_selection(
            target,
            &graph.into_iter().collect::<Vec<_>>(),
            &policies.into_iter().collect::<Vec<_>>(),
        )
    }

    /// Union policy scan roots with source modules selected by Cargo entry
    /// identities. Semantic module lookup reuses this database's canonical tree.
    #[tracing::instrument(level = "debug", skip(self, package))]
    pub(super) fn package_sources(
        &self,
        package: &crate::SemanticPackageContext,
    ) -> BTreeSet<PathBuf> {
        ra_ap_hir::attach_db(&self.db, || {
            let mut files = rust_files(&package.root().join("src"));
            files.extend(package.source_entries().iter().cloned());
            for krate in ra_ap_hir::Crate::all(&self.db) {
                let Some(root) = self.vfs.file_path(krate.root_file(&self.db)).as_path() else {
                    continue;
                };
                if !package
                    .source_entries()
                    .iter()
                    .any(|entry| entry == AsRef::<Path>::as_ref(root))
                {
                    continue;
                }
                for module in krate.modules(&self.db) {
                    let file = module
                        .definition_source_file_id(&self.db)
                        .original_file_respecting_includes(&self.db)
                        .file_id(&self.db);
                    if let Some(path) = self.vfs.file_path(file).as_path() {
                        files.insert(AsRef::<Path>::as_ref(path).to_path_buf());
                    }
                }
            }
            files
        })
    }
}

#[tracing::instrument(level = "debug", skip(root))]
fn rust_files(root: &Path) -> BTreeSet<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect()
}

#[tracing::instrument(level = "trace", skip(path, root))]
fn is_fixture(path: &Path, root: &Path) -> bool {
    #[cfg(any(feature = "_quality_support", feature = "verus_ir"))]
    {
        crate::loader::path_has_fixtures(path, root)
    }
    #[cfg(not(any(feature = "_quality_support", feature = "verus_ir")))]
    {
        let _ = (path, root);
        false
    }
}

#[tracing::instrument(level = "debug", skip(target, graph), err(level = "warn"))]
pub(super) fn target_attributes(target: &CrateTarget, graph: &mut CrateIr) -> CordialResult<()> {
    let root = graph.root();
    if let Some(snapshot) = target.manifest_snapshot() {
        graph.set_attr(
            root,
            crate::ir::ATTR_CARGO_MANIFEST_SNAPSHOT,
            serde_json::to_value(snapshot)?,
        )?;
    }
    graph.set_attr(
        root,
        crate::ir::ATTR_BINARY_ENTRY_PATHS,
        serde_json::to_value(target.binary_entries())?,
    )?;
    if !target.kinds().is_empty() {
        graph.set_attr(
            root,
            crate::ir::ATTR_CRATE_KINDS,
            serde_json::to_value(
                target
                    .kinds()
                    .iter()
                    .map(|kind| kind.as_str())
                    .collect::<Vec<_>>(),
            )?,
        )?;
    }
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
pub(super) fn validate_target(
    target: &CrateTarget,
    context: &crate::SemanticWorkspaceContext,
) -> CordialResult<()> {
    if let Some(snapshot) = target.manifest_snapshot()
        && !context.configuration_inputs().iter().any(|input| {
            input.path() == snapshot.path() && input.digest() == &Some(snapshot.digest())
        })
    {
        return Err(crate::CordialError::semantic_context_input(
            snapshot.path().to_path_buf(),
        ));
    }
    Ok(())
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
impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self, path, sema), err(level = "warn"))]
    pub(super) fn policy_source<'a>(
        &'a self,
        path: &'a Path,
        sema: &ra_ap_hir::Semantics<'_, ra_ap_ide_db::RootDatabase>,
    ) -> CordialResult<crate::loader::DatabaseSource<'a>> {
        use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
        let file = self.file_id(path)?;
        let edition = sema.attach_first_edition(file);
        if let Some(error) = edition.parse(&self.db).errors().first() {
            return Err(anyhow::Error::new(error.clone()).into());
        }
        Ok(crate::loader::DatabaseSource::new(
            #[cfg(any(
                feature = "tracing",
                feature = "internal_error_chain",
                feature = "verus_ir",
                feature = "panics",
                feature = "antipatterns"
            ))]
            &self.db,
            path,
            self.db.file_text(file).text(&self.db),
            sema.parse(edition),
            ra_ap_ide_db::line_index(&self.db, file),
        ))
    }
}
