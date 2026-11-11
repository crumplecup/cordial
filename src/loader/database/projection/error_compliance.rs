//! Collect error compliance from canonical database inputs.

use super::SourceDatabase;
use crate::{CordialResult, ir::SourceErrorCompliance, loader::DatabaseSource};
use ra_ap_hir::Semantics;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self, target), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_error_compliance(
        &self,
        target: &crate::CrateTarget,
    ) -> CordialResult<SourceErrorCompliance> {
        let sema = Semantics::new(&self.db);
        SourceErrorCompliance::collect_with(target.crate_root(), |path, tree| {
            let file = self.file_id(path)?;
            let edition = sema.attach_first_edition(file);
            if let Some(error) = edition.parse(&self.db).errors().first() {
                return Err(anyhow::Error::new(error.clone()).into());
            }
            let source = DatabaseSource::new(
                #[cfg(any(
                    feature = "tracing",
                    feature = "internal_error_chain",
                    feature = "verus_ir",
                    feature = "panics"
                ))]
                &self.db,
                path,
                self.db.file_text(file).text(&self.db),
                sema.parse(edition),
                ra_ap_ide_db::line_index(&self.db, file),
            );
            SourceErrorCompliance::native_file(
                &source,
                tree,
                target.crate_name(),
                target.kinds().contains(&crate::CrateKind::ProcMacro),
            )
        })
    }
}
