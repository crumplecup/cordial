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
        for (_, path) in &paths {
            // The errors alone, which rust-analyzer keeps when it lets the tree go.
            let edition = super::super::edition::edition_of(&self.db, self.file_id(path)?);
            if let Some(error) = edition
                .parse_errors(&self.db)
                .and_then(|errors| errors.first())
            {
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
            tracing::info!(
                target: "cordial::semantic::fallback",
                step = "panics",
                query = "panic_graph or panic_reachable",
                "a query declined: the panics of the target are read file by file"
            );
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
                tracing::info!(
                    target: "cordial::semantic::fallback",
                    step = "panics",
                    query = "panic_scan_file",
                    file = %key.path(&self.db).display(),
                    "a query declined: the panics of the target are read file by file"
                );
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
    #[hotpath::measure]
    pub(in crate::loader::database) fn panic_inputs(
        &self,
        root: &Path,
    ) -> CordialResult<Vec<(PathBuf, PathBuf)>> {
        let held = self.held_files(root)?;
        let mut paths = Vec::new();
        for tree in held.scan_trees(root) {
            for path in held.under(&tree) {
                if !crate::loader::path_has_fixtures(path, root) {
                    paths.push((tree.clone(), path.clone()));
                }
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
            let edition = super::super::edition::edition_of(&self.db, file);
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
