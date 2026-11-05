//! Remaining compatibility collection for internal architecture and apply.

use crate::enricher::error::scan::chain::scan_crate_error_ir_with_source;
use crate::enricher::member_crate_root;
use crate::error::CordialResult;
use crate::hooks::SourceEnrichView;
use tracing::instrument;

#[instrument(level = "debug", skip(view), err(level = "warn"))]
pub(super) fn enrich(view: SourceEnrichView<'_>) -> CordialResult<()> {
    let (ir, source, session) = view.into_parts();

    let crate_root = member_crate_root(source, session);
    let error_config = crate::config::load_session_config(session);
    let kind_fanout_max_call_sites = error_config
        .internal_error_chain()
        .kind_fanout_max_call_sites();
    let field_strategy = error_config.internal_error_chain().field_strategy();
    let report = scan_crate_error_ir_with_source(
        &crate_root,
        ir.crate_name(),
        source,
        kind_fanout_max_call_sites,
        field_strategy,
    )?;
    super::super::compliance::materialize(ir, &crate_root, report.compliance())?;
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
