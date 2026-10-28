//! Present error-policy conversion coverage and claim exact repaired findings.

use std::io::{self, Write};
use std::path::Path;

use crate::{CordialResult, ErrorFieldApplySummary};
use tracing::instrument;

#[instrument(level = "debug", skip(summary), err(level = "warn"))]
pub(super) fn report(
    project_root: &Path,
    summary: &ErrorFieldApplySummary,
    dry_run: bool,
) -> CordialResult<()> {
    writeln!(
        io::stdout(),
        "Error handling apply{}: {} field(s) {} in {} file(s); {} candidate field(s) blocked.",
        if dry_run { " dry run" } else { "" },
        summary.converted_fields(),
        if dry_run {
            "would be converted"
        } else {
            "converted"
        },
        summary.changed_files(),
        summary.unresolved()
    )?;
    for repair in summary.repairs() {
        let path = repair
            .file()
            .strip_prefix(project_root)
            .unwrap_or(repair.file());
        writeln!(
            io::stdout(),
            "{}: {}:{} — {} — {}",
            if dry_run { "Would repair" } else { "Repaired" },
            path.display(),
            repair.line(),
            repair.context(),
            repair.rule_id().as_str()
        )?;
    }
    for blocker in summary.blockers() {
        let path = blocker
            .path()
            .strip_prefix(project_root)
            .unwrap_or(blocker.path());
        writeln!(
            io::stdout(),
            "Blocked: {} — {}::{} — {:?}",
            path.display(),
            blocker.type_name(),
            blocker.field_name(),
            blocker.reason()
        )?;
    }
    Ok(())
}

#[cfg(feature = "quality")]
#[instrument(level = "debug", skip(store, summary), err(level = "warn"))]
pub(super) fn report_worklist(
    project_root: &Path,
    store: &crate::StoreLayout,
    store_home: &Path,
    summary: &ErrorFieldApplySummary,
    dry_run: bool,
) -> CordialResult<()> {
    super::worklist::report(
        project_root,
        store,
        store_home,
        "Error handling",
        |item| item.area() == "Error handling",
        dry_run,
        |item| {
            item.checklist() == "internal-error-chain.checklist.md"
                && summary.repairs().iter().any(|repair| {
                    item.rule_id().as_deref() == Some(repair.rule_id().as_str())
                        && item.crate_name().as_deref() == Some(repair.crate_name())
                        && item.file().as_deref() == repair.file().to_str()
                        && *item.line() == Some(repair.line())
                        && item.item_line().split('`').nth(1) == Some(repair.context())
                })
        },
    )
}
