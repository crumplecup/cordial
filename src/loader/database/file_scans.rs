//! Ask, of every file, what each collector that reads only its tree finds in it, a chunk of files
//! at a time, before the steps that merge the results.
//!
//! Each step asks, for each file in turn, for what one collector found in it. Those are queries
//! keyed by the file (and a projection over one extraction per file, for the collectors that share
//! one: `scan_queries::extract_file`). Asking them here, file by file and in chunks with the
//! syntax trees dropped between the chunks, makes the steps find results ready: a file is parsed
//! once, whatever the capacity of the cache that holds trees, and a restored run, whose results
//! are valid, has nothing to compute here.
//!
//! A file's queries are asked together, on one thread, so that they share its parse. Each collector
//! has its own set of files (the tests are scanned by some and not by others), so a file carries
//! what applies to it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ra_ap_vfs::FileId;
use tracing::instrument;

use super::{SourceDatabase, scan_queries};
use crate::CordialResult;

/// The Rust files under the trees a collector scans, in the order the collectors walk them.
#[instrument(level = "debug", err(level = "warn"))]
#[hotpath::measure]
fn scanned_files(root: &Path) -> CordialResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for tree in crate::loader::quality_scan_trees(root) {
        for entry in walkdir::WalkDir::new(&tree).sort_by_file_name() {
            let entry = entry.map_err(anyhow::Error::new)?;
            if entry.file_type().is_file()
                && entry.path().extension().is_some_and(|ext| ext == "rs")
                && !crate::loader::path_has_fixtures(entry.path(), root)
            {
                files.push(entry.path().to_path_buf());
            }
        }
    }
    Ok(files)
}

/// What is asked of one file: each field is whether a collector reads it, and where it needs the
/// scan tree the file was found under.
#[derive(Debug, Default)]
struct Visit {
    /// The collectors that share the extraction, and the compliance collector.
    scanned: bool,
    #[cfg(feature = "panics")]
    panics: Option<PathBuf>,
    #[cfg(feature = "antipatterns")]
    antipatterns: Option<PathBuf>,
    #[cfg(feature = "antipatterns")]
    contracts: Option<PathBuf>,
    #[cfg(feature = "internal_error_chain")]
    architecture: bool,
    #[cfg(feature = "internal_error_chain")]
    signatures: bool,
}

impl SourceDatabase {
    /// The files each collector reads, joined by file.
    #[instrument(level = "debug", skip(self, target), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn visits(&self, target: &crate::CrateTarget) -> CordialResult<Vec<(FileId, PathBuf, Visit)>> {
        let root = target.crate_root();
        let mut visits: BTreeMap<PathBuf, Visit> = BTreeMap::new();
        for path in scanned_files(root)? {
            visits.entry(path).or_default().scanned = true;
        }
        #[cfg(feature = "panics")]
        for (tree, path) in self.panic_inputs(root)? {
            visits.entry(path).or_default().panics = Some(tree);
        }
        #[cfg(feature = "antipatterns")]
        {
            let contracts =
                crate::etiquettes::antipatterns::antipattern_contract_verifier(target.crate_name())
                    .is_some();
            for (tree, path) in
                crate::etiquettes::antipatterns::native_input_paths(root, contracts)?
            {
                let visit = visits.entry(path.clone()).or_default();
                // Fixture files are read by neither the facts nor the walk.
                if !crate::loader::path_has_fixtures(&path, root) {
                    visit.antipatterns = Some(tree.clone());
                }
                visit.contracts = Some(tree);
            }
        }
        #[cfg(feature = "internal_error_chain")]
        {
            // The files the architecture step reads are those the crate's `mod` declarations lead
            // to, which is found by reading the files' syntax. Those under `src` are a few more, and
            // asking for what is not read costs a collection and nothing else.
            for path in crate::ir::SourceErrorSignatures::input_paths(root)? {
                let visit = visits.entry(path).or_default();
                visit.architecture = true;
                visit.signatures = true;
            }
        }
        Ok(visits
            .into_iter()
            .filter_map(|(path, visit)| self.file_id(&path).ok().map(|file| (file, path, visit)))
            .collect())
    }

    /// Ask, for each file the collectors read, for what they find in it.
    #[instrument(level = "debug", skip(self, target), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn prime_file_scans(&mut self, target: &crate::CrateTarget) -> CordialResult<()> {
        if self
            .cargo_options
            .as_ref()
            .is_some_and(|options| *options.skip_file_scan_priming())
        {
            return Ok(());
        }
        let visits = self.visits(target)?;
        let root = target.crate_root().to_path_buf();
        #[cfg(feature = "internal_error_chain")]
        let proc_macro = target.kinds().contains(&crate::CrateKind::ProcMacro);
        #[cfg(feature = "internal_error_chain")]
        let name = target.crate_name().to_owned();
        #[cfg(feature = "antipatterns")]
        let crate_name = target.crate_name().to_owned();
        self.run_in_chunks("file scans", &visits, &|db, (file, path, visit)| {
            // The compiler does not see that a build with none of the features reads nothing here.
            let _ = (&root, db, file, path, visit);
            if visit.scanned {
                scan_queries::prime_extraction(db, *file, path, &root);
                #[cfg(feature = "internal_error_chain")]
                scan_queries::prime_compliance(db, *file, path, &root, &name, proc_macro);
            }
            #[cfg(feature = "panics")]
            if let Some(tree) = &visit.panics {
                let key = super::crate_files::FileKey::new(
                    db,
                    *file,
                    path.clone(),
                    tree.clone(),
                    root.clone(),
                );
                let _ = super::panic_queries::panic_proofs_file(db, key);
            }
            #[cfg(feature = "antipatterns")]
            {
                if let Some(tree) = &visit.antipatterns {
                    let key = super::crate_files::FileKey::new(
                        db,
                        *file,
                        path.clone(),
                        tree.clone(),
                        root.clone(),
                    );
                    let _ = super::antipattern_queries::antipattern_facts_file(db, key);
                }
                if let Some(tree) = &visit.contracts {
                    let key = super::crate_files::FileKey::new(
                        db,
                        *file,
                        path.clone(),
                        tree.clone(),
                        root.clone(),
                    );
                    let _ = super::antipattern_queries::antipattern_contracts_file(
                        db,
                        super::antipattern_queries::ContractKey::new(db, key, crate_name.clone()),
                    );
                }
            }
            #[cfg(feature = "internal_error_chain")]
            {
                let key = || {
                    super::crate_files::FileKey::new(
                        db,
                        *file,
                        path.clone(),
                        root.join("src"),
                        root.clone(),
                    )
                };
                if visit.architecture {
                    let _ = super::architecture_queries::architecture_file(db, key());
                }
                if visit.signatures {
                    let _ = super::signature_queries::signature_file(db, key());
                }
            }
        });
        Ok(())
    }
}
