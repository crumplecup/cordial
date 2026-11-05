//! Shared architecture catalog collection; policy reads the retained catalog.

use crate::loader::module_path_from_src_file;
use crate::{CordialResult, SharedParsedFile};
use std::path::Path;
use syn::visit::Visit;
use tracing::instrument;

mod catalog;
mod lint;

pub(crate) use catalog::Catalog as ErrorArchitectureCatalog;
use catalog::CatalogVisitor;

impl ErrorArchitectureCatalog {
    #[instrument(level = "debug", skip(self, input), err(level = "warn"))]
    #[hotpath::measure(impl_type = "ErrorArchitectureCatalog")]
    pub(crate) fn collect(
        &mut self,
        input: &SharedParsedFile,
        src_root: &Path,
    ) -> CordialResult<()> {
        let file = input.path();
        let mut visitor = CatalogVisitor::builder()
            .file(file.to_path_buf())
            .source(input)
            .module_prefix(module_path_from_src_file(src_root, file))
            .catalog(self)
            .build()?;
        crate::loader::source_walk(file, "error-architecture", || {
            visitor.visit_file(input.syntax())
        });
        Ok(())
    }
}
