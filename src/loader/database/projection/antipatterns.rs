//! Project antipattern source inventories from canonical database snapshots.
use super::SourceDatabase;
use crate::loader::database::antipattern_queries::{self as queries, WalkKey};
use crate::loader::database::crate_files::{CrateKey, FileKey};
use crate::{CordialResult, SourceAntipatterns};

impl SourceDatabase {
    /// The antipattern inventory of a target, from the per-file queries: a file's
    /// facts, the crate's joined facts, each file's walk and its contract clauses.
    /// A file a query declines sends the whole target down the direct path, which
    /// reports why.
    #[tracing::instrument(level = "debug", skip(self, target), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_antipatterns(
        &self,
        target: &crate::CrateTarget,
    ) -> CordialResult<SourceAntipatterns> {
        let root = target.crate_root();
        let contracts =
            crate::etiquettes::antipatterns::antipattern_contract_verifier(target.crate_name())
                .is_some();
        let paths = crate::etiquettes::antipatterns::native_input_paths(root, contracts)?;
        match self.antipatterns_from_queries(root, target.crate_name(), &paths)? {
            Some(found) => Ok(found),
            None => self.antipatterns_directly(target, root, &paths),
        }
    }

    #[tracing::instrument(level = "debug", skip(self, paths), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn antipatterns_from_queries(
        &self,
        root: &std::path::Path,
        crate_name: &str,
        paths: &[(std::path::PathBuf, std::path::PathBuf)],
    ) -> CordialResult<Option<SourceAntipatterns>> {
        use crate::etiquettes::antipatterns::{AntipatternCatalog, join_clauses};
        let keys: Vec<_> = paths
            .iter()
            .map(|(tree, path)| {
                Ok(FileKey::new(
                    &self.db,
                    self.file_id(path)?,
                    path.clone(),
                    tree.clone(),
                    root.to_path_buf(),
                ))
            })
            .collect::<CordialResult<_>>()?;
        // Fixture files are read by neither the facts nor the walk.
        let ordinary: Vec<_> = keys
            .iter()
            .copied()
            .filter(|key| !crate::loader::path_has_fixtures(key.path(&self.db), root))
            .collect();
        let crate_key = CrateKey::new(
            &self.db,
            ordinary
                .iter()
                .map(|key| {
                    (
                        key.file(&self.db),
                        key.path(&self.db).clone(),
                        key.tree(&self.db).clone(),
                    )
                })
                .collect::<Vec<_>>(),
            root.to_path_buf(),
        );
        let Some(facts) = queries::antipattern_crate_facts(&self.db, crate_key) else {
            tracing::info!(
                target: "cordial::semantic::fallback",
                step = "antipatterns",
                query = "antipattern_crate_facts",
                "a query declined: the antipatterns of the target are read file by file"
            );
            return Ok(None);
        };
        let mut snapshots = Vec::new();
        let mut sites = Vec::new();
        for key in &ordinary {
            let Some(file) = queries::antipattern_sites_file(
                &self.db,
                WalkKey::new(&self.db, *key, facts.clone()),
            ) else {
                tracing::info!(
                    target: "cordial::semantic::fallback",
                    step = "antipatterns",
                    query = "antipattern_sites_file",
                    file = %key.path(&self.db).display(),
                    "a query declined: the antipatterns of the target are read file by file"
                );
                return Ok(None);
            };
            snapshots.push(file.snapshot.clone());
            sites.extend(file.sites.iter().cloned());
        }
        let mut found = Vec::new();
        for key in &keys {
            let contract = queries::antipattern_contracts_file(
                &self.db,
                queries::ContractKey::new(&self.db, *key, crate_name.to_owned()),
            );
            match contract {
                queries::Contracts::Read(file) => found.push(file.clone()),
                queries::Contracts::NotRead => {}
                queries::Contracts::Declined => {
                    tracing::info!(
                        target: "cordial::semantic::fallback",
                        step = "antipatterns",
                        query = "antipattern_contracts_file",
                        file = %key.path(&self.db).display(),
                        "a query declined: the antipatterns of the target are read file by file"
                    );
                    return Ok(None);
                }
            }
        }
        let contracts = join_clauses(root, found.iter(), &mut snapshots);
        Ok(Some(SourceAntipatterns::from_catalog(
            root,
            AntipatternCatalog::from_parts(snapshots, sites, contracts),
        )))
    }

    #[tracing::instrument(level = "debug", skip(self, target, paths), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn antipatterns_directly(
        &self,
        target: &crate::CrateTarget,
        root: &std::path::Path,
        paths: &[(std::path::PathBuf, std::path::PathBuf)],
    ) -> CordialResult<SourceAntipatterns> {
        let inputs: Vec<_> = paths.iter().map(|(_, path)| path.clone()).collect();
        self.collect_sources(&inputs, |sources| {
            let files: Vec<_> = paths
                .iter()
                .zip(sources)
                .map(|((tree, _), source)| (tree.clone(), source))
                .collect();
            SourceAntipatterns::collect(root, target.crate_name(), &files)
        })
    }
}
