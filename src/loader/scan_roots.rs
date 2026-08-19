use std::path::{Path, PathBuf};

use tracing::instrument;
/// Source trees scanned by quality etiquettes (`src/` and `tests/` when present).
#[instrument(level = "debug")]
pub fn quality_scan_trees(crate_root: &Path) -> Vec<PathBuf> {
    ["src", "tests"]
        .into_iter()
        .map(|sub| crate_root.join(sub))
        .filter(|root| root.is_dir())
        .collect()
}

/// Whether `path` lives under this crate's `tests/fixtures/` or `tests/parity/`
/// tree. Those directories are scanned as their own project roots in tests;
/// they are not production sources of the crate under analysis.
///
/// The check is relative to `crate_root`. An absolute `tests/parity` segment
/// anywhere in the path would skip a parity fixture when that fixture *is*
/// the project being scanned.
#[instrument(level = "debug", skip(path))]
pub fn path_has_fixtures(path: &Path, crate_root: &Path) -> bool {
    let relative = path.strip_prefix(crate_root).unwrap_or(path);
    let components: Vec<_> = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();
    components
        .windows(2)
        .any(|window| window[0] == "tests" && (window[1] == "fixtures" || window[1] == "parity"))
}
