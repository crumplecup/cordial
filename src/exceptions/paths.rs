//! Store paths and path normalization for exception files.

use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::store::StoreLayout;

/// Default repo-side registry directory name, relative to the project root.
pub const DEFAULT_EXCEPTIONS_REGISTRY: &str = ".cordial-exceptions";

/// Canonical quality exception file: `{store}/exceptions/{etiquette}/{crate}.json`.
#[instrument(level = "trace", skip(store))]
pub fn exception_file_path(store: &StoreLayout, etiquette_id: &str, crate_name: &str) -> PathBuf {
    store
        .exceptions_dir()
        .join(etiquette_id)
        .join(format!("{crate_name}.json"))
}

/// Canonical coverage skip list: `{store}/patches/{patch_set}.json`.
#[instrument(level = "trace", skip(store))]
pub fn coverage_skip_file_path(store: &StoreLayout, patch_set: &str) -> PathBuf {
    store.patches_dir().join(format!("{patch_set}.json"))
}

/// Resolve a load/backup registry root against the project.
///
/// Absolute paths stay as given. Relative paths join the project root so
/// `cordial -p /repo exceptions load .elicit_doc-exceptions` works from any cwd.
#[instrument(level = "debug")]
pub fn resolve_exceptions_root(project_root: &Path, root: &Path) -> PathBuf {
    if root.is_absolute() {
        root.to_path_buf()
    } else {
        project_root.join(root)
    }
}

#[instrument(level = "debug")]
pub(super) fn paths_match(patch_path: &str, finding_path: &str) -> bool {
    let patch = normalize_rel_path(Path::new(patch_path));
    let finding = normalize_rel_path(Path::new(finding_path));
    patch == finding || finding.ends_with(&patch) || finding.ends_with(&format!("/{patch}"))
}

#[instrument(level = "debug", skip(path))]
pub(super) fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
