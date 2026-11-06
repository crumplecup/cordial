//! Remaining shared-source collection for error apply evidence.

use crate::error::CordialResult;
use crate::hooks::SourceEnrichView;
use tracing::instrument;

#[instrument(level = "debug", skip(view), err(level = "warn"))]
pub(super) fn enrich(view: SourceEnrichView<'_>) -> CordialResult<()> {
    let (ir, source, session) = view.into_parts();

    let error_config = crate::config::load_session_config(session);
    let field_strategy = error_config.internal_error_chain().field_strategy();
    if error_config.etiquette_may_run("internal_error_chain") {
        crate::etiquettes::internal_error_chain::enrich_error_apply_evidence(
            ir,
            source,
            field_strategy,
        )?;
    }
    Ok(())
}
