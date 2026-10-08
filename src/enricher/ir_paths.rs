//! Small IR and path helpers every source-scan enricher shares.

use std::path::{Path, PathBuf};

#[cfg(feature = "_attribute_scan")]
use crate::error::CordialResult;
#[cfg(feature = "_attribute_scan")]
use crate::ir::IrMut;
#[cfg(feature = "_attribute_scan")]
use crate::loader::SourceLoadView;
use crate::session::SessionView;

use tracing::instrument;

#[cfg(feature = "_attribute_scan")]
#[instrument(level = "debug", skip(ir), err(level = "warn"))]
/// The IR node a scan-recorded `context` path hangs under: the node at that
/// path, else its parent module, else the crate root.
pub fn resolve_parent(ir: &dyn IrMut, context: &str) -> CordialResult<crate::ir::NodeId> {
    if context == "<crate>" {
        return ir.root();
    }

    if let Some(node) = ir.node_by_path(context) {
        return Ok(node);
    }

    if let Some((module, _rest)) = context.rsplit_once("::")
        && let Some(node) = ir.node_by_path(module)
    {
        return Ok(node);
    }

    ir.root()
}

/// Crate root directory for a source-loaded member, given its `src/` root.
#[cfg(feature = "_attribute_scan")]
#[instrument(level = "debug", skip(source, session))]
pub fn member_crate_root(source: &SourceLoadView, session: &dyn SessionView) -> PathBuf {
    source
        .src_root()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| session.project_root().to_path_buf())
}

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
