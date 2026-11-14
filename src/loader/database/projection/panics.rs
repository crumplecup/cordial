//! Project panic and proof-exemption facts from the captured database context.
use super::SourceDatabase;
use crate::loader::database::crate_files::{CrateKey, FileKey};
use crate::loader::database::panic_queries as queries;
use crate::{CordialResult, SourcePanics, loader::DatabaseSource};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use std::path::{Path, PathBuf};

impl SourceDatabase {
    /// The panic inventory, from the per-file queries: each file's proof graph,
    /// the crate's joined graph, the functions it reaches, and each file's scan.
    /// A file a query declines sends the crate down the direct path, which
    /// reports why.
    #[tracing::instrument(level = "debug", skip(self, verus), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_panics(
        &self,
        root: &Path,
        #[cfg(feature = "verus_ir")] verus: &crate::VerusCrateIr,
    ) -> CordialResult<SourcePanics> {
        let paths = self.panic_inputs(root)?;
        let sema = Semantics::new(&self.db);
        for (_, path) in &paths {
            let edition = sema.attach_first_edition(self.file_id(path)?);
            if let Some(error) = edition.parse(&self.db).errors().first() {
                return Err(anyhow::Error::new(error.clone()).into());
            }
        }
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
        let crate_key = CrateKey::new(
            &self.db,
            paths
                .iter()
                .zip(&keys)
                .map(|((tree, path), key): (_, &FileKey<'_>)| {
                    (key.file(&self.db), path.clone(), tree.clone())
                })
                .collect::<Vec<_>>(),
            root.to_path_buf(),
        );
        let (Some(graph), Some(reachable)) = (
            queries::panic_graph(&self.db, crate_key),
            queries::panic_reachable(&self.db, crate_key),
        ) else {
            return self.panics_directly(
                root,
                #[cfg(feature = "verus_ir")]
                verus,
            );
        };
        let mut snapshots = Vec::new();
        let mut records = Vec::new();
        for key in &keys {
            let Some(scanned) = queries::panic_scan_file(
                &self.db,
                queries::ScanKey::new(&self.db, *key, reachable.clone()),
            ) else {
                return self.panics_directly(
                    root,
                    #[cfg(feature = "verus_ir")]
                    verus,
                );
            };
            snapshots.push(scanned.snapshot.clone());
            records.extend(scanned.records.iter().cloned());
        }
        #[cfg(feature = "verus_ir")]
        records.extend(crate::etiquettes::panics::verus_findings(verus, root)?);
        Ok(SourcePanics::from_parts(
            root,
            snapshots,
            graph.clone(),
            records,
        ))
    }

    /// The files the panic policy reads, with the scan tree each was found under.
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    fn panic_inputs(&self, root: &Path) -> CordialResult<Vec<(PathBuf, PathBuf)>> {
        let mut paths = Vec::new();
        for tree in crate::loader::quality_scan_trees(root) {
            for entry in walkdir::WalkDir::new(&tree).sort_by_file_name() {
                let entry = entry.map_err(anyhow::Error::new)?;
                let path = entry.path();
                if !entry.file_type().is_file()
                    || path.extension().is_none_or(|ext| ext != "rs")
                    || crate::loader::path_has_fixtures(path, root)
                {
                    continue;
                }
                paths.push((tree.clone(), path.to_path_buf()));
            }
        }
        paths.sort_by(|a, b| a.1.cmp(&b.1));
        Ok(paths)
    }

    #[tracing::instrument(level = "debug", skip(self, verus), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn panics_directly(
        &self,
        root: &Path,
        #[cfg(feature = "verus_ir")] verus: &crate::VerusCrateIr,
    ) -> CordialResult<SourcePanics> {
        let sema = Semantics::new(&self.db);
        let paths = self.panic_inputs(root)?;
        let mut files = Vec::new();
        for (tree, path) in &paths {
            let file = self.file_id(path)?;
            let edition = sema.attach_first_edition(file);
            if let Some(error) = edition.parse(&self.db).errors().first() {
                return Err(anyhow::Error::new(error.clone()).into());
            }
            files.push((
                tree.clone(),
                DatabaseSource::new(
                    &self.db,
                    path,
                    self.db.file_text(file).text(&self.db),
                    sema.parse(edition),
                    ra_ap_ide_db::line_index(&self.db, file),
                ),
            ));
        }
        SourcePanics::collect(
            root,
            &files,
            #[cfg(feature = "verus_ir")]
            verus,
        )
    }
}
