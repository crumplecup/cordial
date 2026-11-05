//! Remaining shared-source collection for error apply evidence.

use crate::enricher::member_crate_root;
use crate::error::CordialResult;
use crate::hooks::SourceEnrichView;
use tracing::instrument;

#[instrument(level = "debug", skip(view), err(level = "warn"))]
pub(super) fn enrich(view: SourceEnrichView<'_>) -> CordialResult<()> {
    let (ir, source, session) = view.into_parts();

    let crate_root = member_crate_root(source, session);
    let error_config = crate::config::load_session_config(session);
    let field_strategy = error_config.internal_error_chain().field_strategy();
    if error_config.etiquette_may_run("internal_error_chain") {
        let mut files = Vec::new();
        crate::etiquettes::internal_error_chain::for_each_shared_src_rust_file(
            source,
            &crate_root,
            |path, _| {
                files.push(source.parsed_file(path, "error-conversion-ir")?);
                Ok(())
            },
        )?;
        crate::etiquettes::internal_error_chain::enrich_error_apply_evidence(
            ir,
            &files,
            field_strategy,
        )?;
    }
    Ok(())
}
