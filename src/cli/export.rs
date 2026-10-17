//! Export command dispatch.

use tracing::instrument;

use super::commands::{ActCtx, ExportCommands};
use super::run::export_surreal;
use crate::CordialResult;

impl ExportCommands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    pub(super) fn act(self, ctx: &ActCtx) -> CordialResult<()> {
        match self {
            Self::Surreal { output, statements } => export_surreal(
                ctx.store(),
                ctx.project_root(),
                ctx.crate_name().as_deref(),
                output.as_deref(),
                statements,
            ),
        }
    }
}
