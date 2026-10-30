//! Show concrete panic conversion obligations and claim only repaired sites.

use crate::{CordialResult, PanicApplySummary};
use std::io::{self, Write};
use std::path::Path;

#[tracing::instrument(level = "debug", skip(summary), err(level = "warn"))]
pub(super) fn report(
    project_root: &Path,
    summary: &PanicApplySummary,
    dry_run: bool,
) -> CordialResult<()> {
    writeln!(
        io::stdout(),
        "Panic apply{}: {} site(s) {}; {} candidate site(s) blocked.",
        if dry_run { " dry run" } else { "" },
        summary.repairs().len(),
        if dry_run {
            "would be converted"
        } else {
            "converted"
        },
        summary.blockers().len()
    )?;
    for repair in summary.repairs() {
        let site = repair.site();
        writeln!(
            io::stdout(),
            "{}: {}:{} — {} — {}",
            if dry_run { "Would repair" } else { "Repaired" },
            site.file()
                .strip_prefix(project_root)
                .unwrap_or(site.file())
                .display(),
            site.line(),
            site.context(),
            site.rule_id()
        )?;
        writeln!(
            io::stdout(),
            "  Owned error: {}, {}, {}; Result signatures: {}",
            repair.error_names().parent(),
            repair.error_names().kind(),
            repair.error_names().source(),
            repair.functions().join(", ")
        )?;
        writeln!(
            io::stdout(),
            "  Failure policy: panic becomes returned error; Rust Result main termination prints Debug and exits with status 1."
        )?;
    }
    for (site, reason) in summary.blockers() {
        writeln!(
            io::stdout(),
            "Blocked: {}:{} — {} — {reason:?}",
            site.file()
                .strip_prefix(project_root)
                .unwrap_or(site.file())
                .display(),
            site.line(),
            site.context()
        )?;
    }
    Ok(())
}

#[cfg(feature = "quality")]
#[tracing::instrument(level = "debug", skip(store, summary), err(level = "warn"))]
pub(super) fn report_worklist(
    project_root: &Path,
    store: &crate::StoreLayout,
    store_home: &Path,
    summary: &PanicApplySummary,
    dry_run: bool,
) -> CordialResult<()> {
    super::worklist::report(
        project_root,
        store,
        store_home,
        "Panic",
        |item| item.checklist() == "panics.checklist.md",
        dry_run,
        |item| {
            summary.repairs().iter().any(|repair| {
                let site = repair.site();
                item.checklist() == "panics.checklist.md"
                    && item.rule_id().as_deref() == Some(site.rule_id())
                    && item.crate_name().as_deref() == Some(site.crate_name())
                    && item.file().as_deref() == site.file().to_str()
                    && *item.line() == Some(site.line())
                    && item.item_line().split('`').nth(1) == Some(site.context())
                    && item.item_line().split('`').nth(5) == Some(".unwrap()")
            })
        },
    )
}
