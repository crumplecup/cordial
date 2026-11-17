//! The digest that names a run's cached results, for tests of what changes it.

use std::path::Path;
use tracing::instrument;

/// The digest of the inputs of a run on `project_root`, hashing its files as a run does, with the
/// digests of files kept in `digests` between calls.
///
/// # Errors
///
/// When the project's packages cannot be listed.
#[instrument(level = "debug", err(level = "warn"))]
#[hotpath::measure]
pub fn run_cache_digest(
    project_root: &Path,
    digests: Option<&Path>,
) -> crate::CordialResult<String> {
    crate::run_cache::gather(&crate::run_cache::KeyRequest::new(
        project_root,
        String::new(),
        digests.map(Path::to_path_buf),
    ))
    .map(|inputs| inputs.digest())
}
