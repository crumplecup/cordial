//! Native workspace splice projection from canonical database inputs.
use super::SourceDatabase;
use crate::CordialResult;
use crate::ir::{PathInclusionFacts, PathInclusionMember, PathInclusionSource};
use crate::loader::database::crate_files::FileKey;
use crate::loader::database::path_inclusion_queries::{self as queries, PackageFileKey};
use ra_ap_hir::{Semantics, attach_db};

impl SourceDatabase {
    /// Project workspace splices and proof calls without compatibility evidence.
    /// Cargo supplies members, enabled dependency edges and entry points; source
    /// selection retains unreferenced src files and native entry module trees.
    /// Missing inputs fail instead of presenting a partial workspace graph.
    #[tracing::instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure]
    pub fn project_path_inclusions(&self) -> CordialResult<PathInclusionFacts> {
        let context = self.projection_context()?;
        if context
            .gaps()
            .contains(&crate::SemanticContextGap::CargoMetadata)
        {
            return Err(crate::CordialError::workspace_source_facts_missing(
                "complete Cargo metadata",
            ));
        }
        let members = context
            .packages()
            .iter()
            .map(|package| {
                PathInclusionMember::new(
                    package.name().clone(),
                    package.root().clone(),
                    package.dependencies().clone(),
                )
            })
            .collect();
        attach_db(&self.db, || {
            let mut sources = Vec::new();
            for package in context.packages() {
                for path in self.package_sources(package) {
                    let id = self.file_id(&path)?;
                    let key = PackageFileKey::new(
                        &self.db,
                        FileKey::new(
                            &self.db,
                            id,
                            path.clone(),
                            package.root().clone(),
                            package.root().clone(),
                        ),
                        package.name().clone(),
                    );
                    match queries::path_inclusion_file(&self.db, key) {
                        Some(found) => sources.push(found.clone()),
                        // The query declines a file that does not parse; reading
                        // it directly says why.
                        None => {
                            sources.push(self.path_inclusion_directly(package.name(), &path, id)?)
                        }
                    }
                }
            }
            Ok(PathInclusionFacts::from_evidence(members, sources))
        })
    }
}

impl SourceDatabase {
    /// One file's source evidence read without the query, to report why the
    /// query declined it.
    #[tracing::instrument(level = "debug", skip(self, package, path, id), err(level = "warn"))]
    #[hotpath::measure]
    fn path_inclusion_directly(
        &self,
        package: &str,
        path: &std::path::Path,
        id: ra_ap_vfs::FileId,
    ) -> CordialResult<PathInclusionSource> {
        let sema = Semantics::new(&self.db);
        let source = super::declarations::source_of(&self.db, &sema, id, path)?;
        Ok(PathInclusionSource::of_file(package, &source))
    }
}
