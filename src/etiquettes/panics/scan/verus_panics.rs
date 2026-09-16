//! Convert `verus_ir` panic sites into panics-etiquette records.
//!
//! A real consumer cannot tell whether a given finding came from the
//! ordinary `syn`-based walk or from `verus_syn`. Kani-reachability
//! exemption does not apply here: `amenable_verus`-shaped crates never
//! mix Kani proof harnesses into the same crate as `verus!` content
//! (verifier backends never depend on each other), so there is nothing
//! for a `verus!` site to be reachable from in the sense `kani_reach`
//! checks. Two Verus-side analogs do apply, though: a site `verus_ir`
//! marked `proven_unreachable_by_ghost_sibling` is that branch's own
//! real, SMT-checked failure mechanism seen only by the ordinary-rustc
//! fallback arm; and a whole function `verus_reach` determines is a
//! verification leaf is itself the checked claim, not library API
//! surface.

use std::path::Path;

use crate::error::CordialResult;
use crate::objects::SourceSpan;

use super::{PanicKind, PanicSiteRecord};
use tracing::instrument;

#[instrument(level = "debug", skip(ir), err(level = "warn"))]
pub(super) fn findings(
    ir: &crate::verus_ir::VerusCrateIr,
    crate_root: &Path,
) -> CordialResult<Vec<PanicSiteRecord>> {
    let reachability = super::super::verus_reach::build_verus_reachability(ir);
    let mut findings = Vec::new();
    for function in ir
        .functions()
        .iter()
        .filter(|function| !reachability.is_verification_leaf(function.name()))
    {
        let context = format!("{}::{}", function.module_path(), function.name());
        let file = function
            .span()
            .file()
            .strip_prefix(crate_root)
            .unwrap_or(function.span().file())
            .to_path_buf();
        let cfg_test = function.cfg_test();
        for site in function
            .panic_sites()
            .iter()
            .filter(|site| !site.proven_unreachable_by_ghost_sibling())
        {
            findings.push(
                PanicSiteRecord::builder()
                    .kind(panic_kind(site.kind()))
                    .context(context.clone())
                    .file(file.clone())
                    .line(site.line())
                    .snippet(site.snippet().clone())
                    .cfg_test(cfg_test)
                    .build()?,
            );
        }
    }
    Ok(findings)
}

#[instrument(level = "debug", skip(kind))]
fn panic_kind(kind: crate::verus_ir::VerusPanicKind) -> PanicKind {
    match kind {
        crate::verus_ir::VerusPanicKind::Panic => PanicKind::Panic,
        crate::verus_ir::VerusPanicKind::Unreachable => PanicKind::Unreachable,
        crate::verus_ir::VerusPanicKind::Expect => PanicKind::Expect,
        crate::verus_ir::VerusPanicKind::Unwrap => PanicKind::Unwrap,
        crate::verus_ir::VerusPanicKind::CompileError => PanicKind::CompileError,
    }
}
