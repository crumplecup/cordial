//! Apply only migrations whose declarations, callers, and entry boundary close.

use std::path::Path;
use std::sync::Arc;

use crate::{
    ClosedPanicDecision, CordialResult, Disposition, IrView, NamedRunFilter, ProgressSink,
    SessionBuilder, StoreLayout,
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
        let mut candidates = Vec::new();
        for finding in findings
            .iter()
            .filter(|finding| finding.disposition() == Disposition::Open)
        {
            let site = candidate::site(&ir, finding.as_ref())?;
            if config.internal_error_chain().field_strategy() != crate::ErrorFieldStrategy::Owned {
                summary.block(site, PanicApplyBlocker::BorrowedPolicy);
                continue;
            }
            match candidate::selection(&ir, finding.as_ref())? {
                Ok(selection) => candidates.push((site, selection)),
                Err(reason) => summary.block(site, reason),
            }
        }
        if candidates.is_empty() {
            continue;
        }
        // Compose every assessed selection before the single closed write.
        let selections: Vec<_> = candidates.iter().map(|(_, selection)| *selection).collect();
        let decision = if let Some(main) = ir.node_by_path("main") {
            match crate::plan_closed_owned_panic_group(&ir, &selections, main)? {
                ClosedPanicDecision::Ready(migration) => Ok(migration),
                ClosedPanicDecision::Refused(reason) => Err(PanicApplyBlocker::Closure(reason)),
                ClosedPanicDecision::NoCandidate => Err(PanicApplyBlocker::UnsupportedSite),
            }
        } else {
            Err(PanicApplyBlocker::MissingEntryPoint)
        };
        let migration = match decision {
            Ok(migration) => migration,
            Err(reason) => {
                for (site, _) in candidates {
                    summary.block(site, reason.clone());
                }
                continue;
            }
        };
        let functions = candidate::function_names(&ir, &migration)?;
        crate::apply_closed_panic_migration(&migration, dry_run)?;
        for (site, _) in candidates {
            summary.repair(PanicApplyRepair::new(
                site,
                functions.clone(),
                migration.component().error_names().clone(),
            ));
        }
    }
    Ok(summary)
}
