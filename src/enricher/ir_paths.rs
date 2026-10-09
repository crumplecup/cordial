//! Small IR and path helpers every source-scan enricher shares.

use std::path::{Path, PathBuf};

use crate::session::SessionView;

use tracing::instrument;

/// Resolves a scan-recorded (possibly relative) source path against the
/// project root, for findings whose scan step ran outside session context.
#[instrument(level = "debug", skip(session, path))]
pub fn resolve_source_path(session: &dyn SessionView, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        session.project_root().join(path)
    }
}
