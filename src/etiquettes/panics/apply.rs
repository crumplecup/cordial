//! Apply only migrations whose declarations, callers, and entry boundary close.

use std::path::Path;
use std::sync::Arc;

use crate::{
    CordialResult, Disposition, NamedRunFilter, ProgressSink, SessionBuilder, StoreLayout,
};

mod candidate;
mod types;
pub use types::{PanicApplyBlocker, PanicApplyRepair, PanicApplySite, PanicApplySummary};

/// Plan from fresh shared IR and write each supported closed binary component.
///
/// Successful behavior is retained. Panic failure becomes a returned owned
/// error, observed by Rust's Result main termination. Dry runs validate the
/// same write preconditions without changing source or claiming worklist items.
#[tracing::instrument(level = "debug", skip(store, progress), err(level = "warn"))]
#[hotpath::measure]
pub fn run_panic_apply(
    project_root: &Path,
    store: &StoreLayout,
    store_home: &Path,
    only_crate: Option<&str>,
    dry_run: bool,
    progress: Arc<dyn ProgressSink>,
) -> CordialResult<PanicApplySummary> {
    let session = SessionBuilder::new(project_root)
        .with_store_root(store.root())
        .with_store_home(store_home)
        .with_progress_sink(progress)
        .register(&*super::PANICS_ETIQUETTE)
        .build();
    let mut filter = NamedRunFilter::etiquettes(["panics"]);
    if let Some(name) = only_crate {
        filter = filter.with_crate(name);
    }
    let config = crate::load_session_config(&session);
    let evidence = crate::session::collect_apply_evidence(&session, &filter)?;
    let mut summary = PanicApplySummary::default();
    for (crate_name, findings) in evidence.findings() {
        let ir = crate::CrateView::new(evidence.workspace(), crate_name.clone());
        let mut plans = Vec::new();
        for finding in findings
            .iter()
            .filter(|finding| finding.disposition() == Disposition::Open)
        {
            let site = candidate::site(&ir, finding.as_ref())?;
            if config.internal_error_chain().field_strategy() != crate::ErrorFieldStrategy::Owned {
                summary.block(site, PanicApplyBlocker::BorrowedPolicy);
                continue;
            }
            match candidate::plan(&ir, finding.as_ref())? {
                Ok(migration) => plans.push((site, migration)),
                Err(reason) => summary.block(site, reason),
            }
        }
        // All plans were made against one snapshot. Never apply a second plan
        // against bytes changed by the first or silently omit a shared caller.
        if plans.len() > 1 {
            for (site, _) in plans {
                summary.block(site, PanicApplyBlocker::OverlappingMigrations);
            }
            continue;
        }
        for (site, migration) in plans {
            let functions = candidate::function_names(&ir, &migration)?;
            crate::apply_closed_panic_migration(&migration, dry_run)?;
            summary.repair(PanicApplyRepair::new(
                site,
                functions,
                migration.component().error_names().clone(),
            ));
        }
    }
    Ok(summary)
}
