//! Collect error compliance from canonical database inputs.

use super::SourceDatabase;
use crate::loader::database::scan_queries::{self, ScanKey};
use crate::{CordialResult, ir::SourceErrorCompliance};
use ra_ap_hir::Semantics;

impl SourceDatabase {
    #[tracing::instrument(level = "debug", skip(self, target), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn project_error_compliance(
        &self,
        target: &crate::CrateTarget,
    ) -> CordialResult<SourceErrorCompliance> {
        SourceErrorCompliance::collect_with(target.crate_root(), |path, tree| {
            let key = ScanKey::new(
                &self.db,
                self.file_id(path)?,
                path.to_path_buf(),
                tree.to_path_buf(),
                target.crate_root().to_path_buf(),
                target.crate_name().to_owned(),
                target.kinds().contains(&crate::CrateKind::ProcMacro),
            );
            match scan_queries::error_compliance_file(&self.db, key) {
                Some(found) => Ok(found.clone()),
                None => {
                    let sema = Semantics::new(&self.db);
                    let (_, source) = self.declaration_source(&sema, path)?;
                    SourceErrorCompliance::native_file(
                        &source,
                        tree,
                        target.crate_name(),
                        target.kinds().contains(&crate::CrateKind::ProcMacro),
                    )
                }
            }
        })
    }
}
