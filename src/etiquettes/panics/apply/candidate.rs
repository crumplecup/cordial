//! Join assessed panic anchors to located function facts without a source scan.

use super::{PanicApplyBlocker, PanicApplySite};
use crate::{
    ClosedPanicDecision, ClosedPanicMigration, EdgeKind, plan_closed_owned_panic_component,
};
use crate::{CordialError, CordialResult, Finding, IrView, SourceSpan};

#[tracing::instrument(level = "trace", skip(ir, finding), err(level = "warn"))]
pub(super) fn site(ir: &dyn IrView, finding: &dyn Finding) -> CordialResult<PanicApplySite> {
    let node = ir
        .node(finding.anchor().node_id())
        .ok_or_else(missing_evidence)?;
    let span = node.weight().span().as_ref().ok_or_else(missing_evidence)?;
    let context = node
        .attr("context")
        .and_then(|value| value.as_str())
        .ok_or_else(missing_evidence)?;
    if span.line() == 0 {
        return Err(missing_evidence());
    }
    Ok(PanicApplySite::new(
        ir.crate_name().to_owned(),
        span.file().to_path_buf(),
        span.line(),
        context.to_owned(),
        finding.rule().id().to_owned(),
    ))
}

#[tracing::instrument(level = "debug", skip(ir, finding), err(level = "warn"))]
pub(super) fn plan(
    ir: &dyn IrView,
    finding: &dyn Finding,
) -> CordialResult<Result<ClosedPanicMigration, PanicApplyBlocker>> {
    use PanicApplyBlocker as Blocker;
    let anchor = finding.anchor().node_id();
    let node = ir.node(anchor).ok_or_else(missing_evidence)?;
    if finding.rule().id() != "PANIC-SOURCE-UNWRAP"
        || node.attr("snippet").and_then(|value| value.as_str()) != Some(".unwrap()")
    {
        return Ok(Err(Blocker::UnsupportedSite));
    }
    let site = site(ir, finding)?;
    let parents = ir.parents(anchor, EdgeKind::Contains);
    let [function] = parents.as_slice() else {
        return Ok(Err(Blocker::UnsupportedSite));
    };
    let Some(facts) = ir.function_source_syntax(*function)? else {
        return Ok(Err(Blocker::UnsupportedSite));
    };
    let indices: Vec<_> = facts
        .unwraps()
        .iter()
        .enumerate()
        .filter(|(_, unwrap)| unwrap.line() == Some(site.line()))
        .map(|(index, _)| index)
        .collect();
    let [index] = indices.as_slice() else {
        return Ok(Err(Blocker::UnsupportedSite));
    };
    if !ir.node(*function).is_some_and(|node| {
        node.weight()
            .span()
            .as_ref()
            .is_some_and(|span| span.file() == site.file())
    }) {
        return Ok(Err(Blocker::UnsupportedSite));
    }
    let Some(main) = ir.node_by_path("main") else {
        return Ok(Err(Blocker::MissingEntryPoint));
    };
    Ok(
        match plan_closed_owned_panic_component(ir, *function, *index, main)? {
            ClosedPanicDecision::Ready(migration) => Ok(migration),
            ClosedPanicDecision::Refused(reason) => Err(Blocker::Closure(reason)),
            ClosedPanicDecision::NoCandidate => Err(Blocker::UnsupportedSite),
        },
    )
}

#[tracing::instrument(level = "trace", skip(ir, migration), err(level = "warn"))]
pub(super) fn function_names(
    ir: &dyn IrView,
    migration: &ClosedPanicMigration,
) -> CordialResult<Vec<String>> {
    migration
        .component()
        .functions()
        .iter()
        .map(|id| {
            ir.node(*id)
                .and_then(|node| node.weight().name().clone())
                .ok_or_else(missing_evidence)
        })
        .collect()
}

#[tracing::instrument(level = "trace")]
fn missing_evidence() -> CordialError {
    CordialError::workspace_source_facts_missing("panic apply anchor and source coordinates")
}
