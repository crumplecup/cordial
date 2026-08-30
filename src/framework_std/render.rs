//! Framework std coverage artifact writers.

use std::fmt::Write as _;

use crate::csv_row::csv_field as csv_escape;
use crate::error::CordialResult;
use crate::framework_std::{
    FrameworkGapEntry, FrameworkTraitReport, FrameworkTraitStatus, SkipMap,
};

use tracing::instrument;
#[instrument(level = "debug", skip(report), err(level = "warn"))]
pub fn render_framework_coverage_csv(report: &FrameworkTraitReport) -> CordialResult<String> {
    let mut body = String::from("type_path,type_kind,is_generic,trait_status,skip_reason\n");
    let mut rows: Vec<_> = report.entries.iter().collect();
    rows.sort_by(|left, right| left.type_path.cmp(&right.type_path));
    for entry in rows {
        writeln!(
            body,
            "{},{},{},{},{}",
            csv_escape(&entry.type_path),
            entry.type_kind,
            entry.is_generic,
            entry.trait_status,
            csv_escape(entry.skip_reason.as_deref().unwrap_or(""))
        )?;
    }
    Ok(body)
}

#[instrument(level = "debug", skip(gaps), err(level = "warn"))]
pub fn render_framework_gaps_csv(gaps: &[FrameworkGapEntry]) -> CordialResult<String> {
    let mut body = String::from("source_crate,type_path,type_kind,trait_name,impl_crate,action\n");
    for gap in gaps {
        writeln!(
            body,
            "{},{},{},{},{},{}",
            csv_escape(&gap.source_crate),
            csv_escape(&gap.type_path),
            gap.type_kind,
            csv_escape(&gap.trait_name),
            csv_escape(&gap.impl_crate),
            csv_escape(&gap.action)
        )?;
    }
    Ok(body)
}

#[instrument(level = "debug", skip(report, skip_map), err(level = "warn"))]
pub fn render_framework_checklist_md(
    report: &FrameworkTraitReport,
    skip_map: &SkipMap,
) -> CordialResult<String> {
    let missing: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| entry.trait_status == FrameworkTraitStatus::Missing)
        .collect();
    let skipped: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| entry.trait_status == FrameworkTraitStatus::Skipped)
        .collect();

    let accountable = report.entries.len().saturating_sub(report.skipped_count);
    let mut out = String::new();
    writeln!(
        out,
        "# {} `{}` coverage checklist\n",
        report.source_crate, report.trait_name
    )?;
    writeln!(
        out,
        "**Impl crate:** `{}`  \n**Scope:** {}  \n**Accountable types:** {}  \n**Complete:** {} ({:.1}%)  \n**Missing:** {}  \n**Skipped (patched):** {}\n",
        report.impl_crate,
        if report.include_nightly {
            "stable + nightly std types"
        } else {
            "stable std types only"
        },
        accountable,
        report.complete_count,
        report.coverage_pct(),
        report.missing_count,
        report.skipped_count,
    )?;

    writeln!(
        out,
        "## Missing `{}` impl ({})",
        report.trait_name,
        missing.len()
    )?;
    if missing.is_empty() {
        writeln!(
            out,
            "\n_All accountable types have `{}` impls._\n",
            report.trait_name
        )?;
    } else {
        out.push('\n');
        for entry in missing {
            writeln!(
                out,
                "- [ ] `{}` ({}) — add in `{}`",
                entry.type_path, entry.type_kind, report.impl_crate
            )?;
        }
        out.push('\n');
    }

    writeln!(out, "## Documented exceptions ({})", skipped.len())?;
    if skipped.is_empty() {
        writeln!(
            out,
            "\n_No patch entries. Add `{{store}}/patches/homecoming.json` to document intentional exclusions._\n"
        )?;
    } else {
        out.push('\n');
        for entry in skipped {
            let reason = entry
                .skip_reason
                .as_deref()
                .or_else(|| skip_map.get(&entry.type_path).map(String::as_str))
                .unwrap_or("documented in patch set");
            writeln!(out, "- `{}` — {}", entry.type_path, reason)?;
        }
        out.push('\n');
    }
    Ok(out)
}

#[instrument(level = "debug", skip(report))]
pub fn render_framework_summary_md(report: &FrameworkTraitReport) -> String {
    let accountable = report.entries.len().saturating_sub(report.skipped_count);
    format!(
        "# Framework trait coverage summary\n\n\
        **Profile:** std type list vs `{trait}` in `{impl_crate}`  \n\
        **Scope:** {scope}  \n\
        **Source inventory:** `{source}` (std + core + alloc)  \n\
        **Total types:** {total}  \n\
        **Accountable:** {accountable}  \n\
        **Complete:** {complete} ({pct:.1}%)  \n\
        **Missing:** {missing}  \n\
        **Skipped:** {skipped}\n\n\
        Open `std.checklist.md` for the actionable gap list.\n",
        trait = report.trait_name,
        impl_crate = report.impl_crate,
        scope = if report.include_nightly {
            "stable + nightly std types"
        } else {
            "stable std types only"
        },
        source = report.source_crate,
        total = report.entries.len(),
        accountable = accountable,
        complete = report.complete_count,
        pct = report.coverage_pct(),
        missing = report.missing_count,
        skipped = report.skipped_count,
    )
}
