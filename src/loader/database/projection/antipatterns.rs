//! Project antipattern source inventories from canonical database snapshots.
use super::SourceDatabase;
use crate::{CordialResult, SourceAntipatterns, loader::DatabaseSource};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self, target), err(level = "warn"))]
    pub(super) fn project_antipatterns(
        &self,
        target: &crate::CrateTarget,
    ) -> CordialResult<SourceAntipatterns> {
        let root = target.crate_root();
        let contracts =
            crate::etiquettes::antipatterns::antipattern_contract_verifier(target.crate_name())
                .is_some();
        let sema = Semantics::new(&self.db);
        let mut paths = Vec::new();
        for tree in crate::loader::quality_scan_trees(root) {
            for entry in walkdir::WalkDir::new(&tree).sort_by_file_name() {
                let entry = entry.map_err(anyhow::Error::new)?;
                let path = entry.path();
                if !entry.file_type().is_file()
                    || path.extension().is_none_or(|ext| ext != "rs")
                    || (crate::loader::path_has_fixtures(path, root)
                        && !(contracts
                            && path.starts_with(root.join("src"))
                            && !path.components().any(|part| part.as_os_str() == "gallery")))
                {
                    continue;
                }
                paths.push((tree.clone(), path.to_path_buf()));
            }
        }
        paths.sort_by(|a, b| a.1.cmp(&b.1));
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
        SourceAntipatterns::collect_native(root, target.crate_name(), &files)
    }
}
