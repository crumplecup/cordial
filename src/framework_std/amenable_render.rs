//! Amenable std registry coverage artifact writers.

use std::fmt::Write as _;

use crate::csv_row::csv_field as csv_escape;
use crate::error::CordialResult;
use crate::framework_std::amenable::{AmenableStdGapEntry, AmenableStdReport, AmenableStdStatus};
use crate::framework_std::verifier_skip::VerifierSkipMap;

use tracing::instrument;
#[instrument(level = "debug", skip(report), err(level = "warn"))]
pub fn render_amenable_std_coverage_csv(report: &AmenableStdReport) -> CordialResult<String> {
    let mut body = String::from(
        "type_path,type_kind,is_generic,status,evidence_link,evidence_name,kani_witness,creusot_witness,verus_witness,proof_test,skip_reason,kind,parent,note\n",
    );
    let mut rows: Vec<_> = report.entries().iter().collect();
    rows.sort_by(|left, right| left.type_path().cmp(right.type_path()));
    for entry in rows {
        writeln!(
            body,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            csv_escape(entry.type_path()),
            entry.type_kind(),
            entry.is_generic(),
            entry.status(),
            entry.evidence_link(),
            csv_escape(entry.evidence_name().as_deref().unwrap_or("")),
            entry.kani_witness(),
            entry.creusot_witness(),
            entry.verus_witness(),
            entry.proof_test(),
            csv_escape(entry.skip_reason().as_deref().unwrap_or("")),
            entry.kind(),
            csv_escape(entry.parent().as_deref().unwrap_or("")),
            csv_escape(entry.note().as_deref().unwrap_or(""))
        )?;
    }
    Ok(body)
}

#[instrument(level = "debug", skip(gaps), err(level = "warn"))]
pub fn render_amenable_std_gaps_csv(gaps: &[AmenableStdGapEntry]) -> CordialResult<String> {
    let mut body = String::from("source_crate,type_path,type_kind,status,missing_layers,action\n");
    for gap in gaps {
        writeln!(
            body,
            "{},{},{},{},{},{}",
            csv_escape(gap.source_crate()),
            csv_escape(gap.type_path()),
            gap.type_kind(),
            gap.status(),
            csv_escape(gap.missing_layers()),
            csv_escape(gap.action())
        )?;
    }
    Ok(body)
}

/// Amenable std registry coverage checklist.
#[instrument(level = "debug", skip(report, skip_map), err(level = "warn"))]
pub fn render_amenable_std_checklist_md(
    report: &AmenableStdReport,
    skip_map: &VerifierSkipMap,
) -> CordialResult<String> {
    render_wrapped_checklist_md(
        report,
        skip_map,
        "Amenable std",
        "RustStdStandard<T>",
        "amenable",
    )
}

/// Amenable ext (third-party crate) registry coverage checklist.
#[instrument(level = "debug", skip(report, skip_map), err(level = "warn"))]
pub fn render_amenable_ext_checklist_md(
    report: &AmenableStdReport,
    skip_map: &VerifierSkipMap,
    patch_set: &str,
) -> CordialResult<String> {
    render_wrapped_checklist_md(
        report,
        skip_map,
        "Amenable ext",
        "ExtStandard<T>",
        patch_set,
    )
}

/// Registry coverage checklist, naming `profile_title` in its heading,
/// `wrapper_type` in its "no missing evidence" note, and `patch_set` in
/// its "no patch entries" note (matching `load_verifier_skip_map`'s own
/// `{patch_set}.json` file-naming convention) — shared by
/// [`render_amenable_std_checklist_md`] and
/// [`render_amenable_ext_checklist_md`]. Every other piece of text
/// (scope, impl crate, source crate) already reads from `report` itself
/// rather than assuming "std", so no further parameterization is needed.
#[instrument(level = "debug", skip(report, skip_map))]
fn render_wrapped_checklist_md(
    report: &AmenableStdReport,
    skip_map: &VerifierSkipMap,
    profile_title: &str,
    wrapper_type: &str,
    patch_set: &str,
) -> CordialResult<String> {
    // A generic row with instantiation rows is open only because they are;
    // the instantiation rows carry the action, so the aggregate is left out
    // of the two action lists and shown in its own section.
    let actionable = |status: AmenableStdStatus| -> Vec<_> {
        report
            .entries()
            .iter()
            .filter(|entry| entry.status() == status && entry.kind() != "aggregate")
            .collect()
    };
    let missing = actionable(AmenableStdStatus::Missing);
    let partial = actionable(AmenableStdStatus::Partial);
    let aggregates: Vec<_> = report
        .entries()
        .iter()
        .filter(|entry| entry.kind() == "aggregate")
        .collect();
    let documented: Vec<_> = report
        .entries()
        .iter()
        .filter(|entry| entry.skip_reason().is_some())
        .collect();

    let accountable = report
        .entries()
        .len()
        .saturating_sub(report.skipped_count());
    let mut out = String::new();
    writeln!(out, "# {profile_title} registry coverage checklist\n")?;
    writeln!(
        out,
        "**Source crate:** `{}`  \n**Impl crate:** `{}`  \n**Scope:** {}  \n**Accountable types:** {}  \n**Complete (evidence + all witnesses):** {} ({:.1}%)  \n**Partial:** {}  \n**Missing evidence:** {}  \n**Skipped (patched):** {}\n",
        report.source_crate(),
        report.impl_crate(),
        if report.include_nightly() {
            format!("stable + nightly {} types", report.source_crate())
        } else {
            format!(
                "stable {} types only (pass `--include-nightly` for unstable items)",
                report.source_crate()
            )
        },
        accountable,
        report.complete_count(),
        report.coverage_pct(),
        report.partial_count(),
        report.missing_count(),
        report.skipped_count(),
    )?;

    writeln!(out, "## Missing evidence link ({})", missing.len())?;
    if missing.is_empty() {
        writeln!(
            out,
            "\n_All accountable {} types have `{wrapper_type}` evidence links._\n",
            report.source_crate()
        )?;
    } else {
        writeln!(out)?;
        for entry in missing {
            writeln!(
                out,
                "- [ ] `{}` ({}){} — register in `{}`",
                entry.type_path(),
                entry.type_kind(),
                instantiation_suffix(entry),
                report.impl_crate()
            )?;
        }
        writeln!(out)?;
    }

    writeln!(out, "## Partial witness coverage ({})", partial.len())?;
    if partial.is_empty() {
        writeln!(
            out,
            "\n_No partial rows — every registered type has kani, creusot, and verus proofs._\n"
        )?;
    } else {
        writeln!(out)?;
        for entry in partial {
            let mut gaps = Vec::new();
            if !entry.kani_witness() && !entry.kani_excepted() {
                gaps.push("kani");
            }
            if !entry.creusot_witness() && !entry.creusot_excepted() {
                gaps.push("creusot");
            }
            if !entry.verus_witness() && !entry.verus_excepted() {
                gaps.push("verus");
            }
            if !entry.proof_test() {
                gaps.push("proof_test");
            }
            writeln!(
                out,
                "- [ ] `{}`{} — missing: {}",
                entry.type_path(),
                instantiation_suffix(entry),
                gaps.join(", ")
            )?;
        }
        writeln!(out)?;
    }

    if !aggregates.is_empty() {
        writeln!(
            out,
            "## Generic types and their instantiations ({})",
            aggregates.len()
        )?;
        for aggregate in aggregates {
            writeln!(
                out,
                "\n### `{}` — {}\n",
                aggregate.type_path(),
                aggregate.status()
            )?;
            if let Some(note) = aggregate.note() {
                writeln!(out, "{note}.\n")?;
            }
            for child in report
                .entries()
                .iter()
                .filter(|entry| entry.parent().as_deref() == Some(aggregate.type_path().as_str()))
            {
                writeln!(out, "- `{}` — {}", child.type_path(), child.status())?;
            }
        }
        writeln!(out)?;
    }

    writeln!(out, "## Documented exceptions ({})", documented.len())?;
    if documented.is_empty() {
        writeln!(
            out,
            "\n_No patch entries. Add `~/.cordial/{{project}}/patches/{patch_set}.json` to document intentional exclusions._\n"
        )?;
    } else {
        writeln!(out)?;
        for entry in documented {
            let reason = entry
                .skip_reason()
                .as_deref()
                .or_else(|| skip_map.get(entry.type_path()).map(|e| e.reason().as_str()))
                .unwrap_or("documented in patch set");
            if entry.status() == AmenableStdStatus::Skipped {
                writeln!(out, "- `{}` — {}", entry.type_path(), reason)?;
            } else {
                writeln!(
                    out,
                    "- `{}` ({}, scoped exception) — {}",
                    entry.type_path(),
                    entry.status(),
                    reason
                )?;
            }
        }
        writeln!(out)?;
    }

    Ok(out)
}

/// " (instantiation of `parent`)" for an instantiation row, else nothing.
#[instrument(level = "trace", skip(entry))]
fn instantiation_suffix(entry: &crate::framework_std::amenable::AmenableStdEntry) -> String {
    entry
        .parent()
        .as_deref()
        .map(|parent| format!(" (instantiation of `{parent}`)"))
        .unwrap_or_default()
}

/// Amenable std registry coverage summary.
#[instrument(level = "debug", skip(report))]
pub fn render_amenable_std_summary_md(report: &AmenableStdReport) -> String {
    render_wrapped_summary_md(
        report,
        "Amenable std",
        "RustStdStandard<T>",
        " (std + core + alloc)",
        "std.checklist.md",
    )
}

/// Amenable ext (third-party crate) registry coverage summary.
#[instrument(level = "debug", skip(report))]
pub fn render_amenable_ext_summary_md(
    report: &AmenableStdReport,
    checklist_filename: &str,
) -> String {
    render_wrapped_summary_md(
        report,
        "Amenable ext",
        "ExtStandard<T>",
        "",
        checklist_filename,
    )
}

/// Registry coverage summary, naming `profile_title`/`wrapper_type` and
/// pointing at `checklist_filename` — shared by
/// [`render_amenable_std_summary_md`] and
/// [`render_amenable_ext_summary_md`]. `source_note` is appended
/// verbatim after the source-inventory crate name (`" (std + core +
/// alloc)"` for std, empty for a single-crate ext target).
#[instrument(level = "debug", skip(report))]
fn render_wrapped_summary_md(
    report: &AmenableStdReport,
    profile_title: &str,
    wrapper_type: &str,
    source_note: &str,
    checklist_filename: &str,
) -> String {
    let accountable = report
        .entries()
        .len()
        .saturating_sub(report.skipped_count());
    format!(
        "# {profile_title} registry coverage summary\n\n\
        **Profile:** {source} type list vs `{wrapper_type}` evidence + verifier witnesses  \n\
        **Scope:** {scope}  \n\
        **Impl crate:** `{impl_crate}`  \n\
        **Source inventory:** `{source}`{source_note}  \n\
        **Total types:** {total}  \n\
        **Accountable:** {accountable}  \n\
        **Complete:** {complete} ({pct:.1}%)  \n\
        **Partial:** {partial}  \n\
        **Missing evidence:** {missing}  \n\
        **Skipped:** {skipped}\n\n\
        Open `{checklist_filename}` for the actionable gap list.\n",
        scope = if report.include_nightly() {
            format!("stable + nightly {} types", report.source_crate())
        } else {
            format!("stable {} types only", report.source_crate())
        },
        impl_crate = report.impl_crate(),
        source = report.source_crate(),
        total = report.entries().len(),
        accountable = accountable,
        complete = report.complete_count(),
        pct = report.coverage_pct(),
        partial = report.partial_count(),
        missing = report.missing_count(),
        skipped = report.skipped_count(),
    )
}
