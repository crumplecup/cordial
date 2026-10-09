//! IR-parent and crate-root helpers for the attribute scanner and the
//! enrichers built beside it.

use std::path::{Path, PathBuf};

use crate::error::CordialResult;
use crate::ir::IrMut;
use crate::loader::SourceLoadView;
use crate::session::SessionView;

use tracing::instrument;

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
#[instrument(level = "debug", skip(source, session))]
pub fn member_crate_root(source: &SourceLoadView, session: &dyn SessionView) -> PathBuf {
    source
        .src_root()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| session.project_root().to_path_buf())
}
