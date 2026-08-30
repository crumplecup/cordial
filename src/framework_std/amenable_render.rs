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
        "type_path,type_kind,is_generic,status,evidence_link,evidence_name,kani_witness,creusot_witness,verus_witness,proof_test,skip_reason\n",
    );
    let mut rows: Vec<_> = report.entries.iter().collect();
    rows.sort_by(|left, right| left.type_path.cmp(&right.type_path));
    for entry in rows {
        writeln!(
            body,
            "{},{},{},{},{},{},{},{},{},{},{}",
            csv_escape(&entry.type_path),
            entry.type_kind,
            entry.is_generic,
            entry.status,
            entry.evidence_link,
            csv_escape(entry.evidence_name.as_deref().unwrap_or("")),
            entry.kani_witness,
            entry.creusot_witness,
            entry.verus_witness,
            entry.proof_test,
            csv_escape(entry.skip_reason.as_deref().unwrap_or(""))
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
            csv_escape(&gap.source_crate),
            csv_escape(&gap.type_path),
            gap.type_kind,
            gap.status,
            csv_escape(&gap.missing_layers),
            csv_escape(&gap.action)
        )?;
    }
    Ok(body)
}

#[instrument(level = "debug", skip(report, skip_map), err(level = "warn"))]
pub fn render_amenable_std_checklist_md(
    report: &AmenableStdReport,
    skip_map: &VerifierSkipMap,
) -> CordialResult<String> {
    let missing: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| entry.status == AmenableStdStatus::Missing)
        .collect();
    let partial: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| entry.status == AmenableStdStatus::Partial)
        .collect();
    let documented: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| entry.skip_reason.is_some())
        .collect();

    let accountable = report.entries.len().saturating_sub(report.skipped_count);
    let mut out = String::from("# Amenable std registry coverage checklist\n\n");
    writeln!(
        out,
        "**Impl crate:** `{}`  \n**Scope:** {}  \n**Accountable types:** {}  \n**Complete (evidence + all witnesses):** {} ({:.1}%)  \n**Partial:** {}  \n**Missing evidence:** {}  \n**Skipped (patched):** {}\n",
        report.impl_crate,
        if report.include_nightly {
            "stable + nightly std types"
        } else {
            "stable std types only (pass `--include-nightly` for unstable items)"
        },
        accountable,
        report.complete_count,
        report.coverage_pct(),
        report.partial_count,
        report.missing_count,
        report.skipped_count,
    )?;

    writeln!(out, "## Missing evidence link ({})", missing.len())?;
    if missing.is_empty() {
        writeln!(
            out,
            "\n_All accountable std types have `RustStdStandard<T>` evidence links._\n"
        )?;
    } else {
        writeln!(out)?;
        for entry in missing {
            writeln!(
                out,
                "- [ ] `{}` ({}) — register in `{}`",
                entry.type_path, entry.type_kind, report.impl_crate
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
            if !entry.kani_witness && !entry.kani_excepted {
                gaps.push("kani");
            }
            if !entry.creusot_witness && !entry.creusot_excepted {
                gaps.push("creusot");
            }
            if !entry.verus_witness && !entry.verus_excepted {
                gaps.push("verus");
            }
            if !entry.proof_test {
                gaps.push("proof_test");
            }
            writeln!(
                out,
                "- [ ] `{}` — missing: {}",
                entry.type_path,
                gaps.join(", ")
            )?;
        }
        writeln!(out)?;
    }

    writeln!(out, "## Documented exceptions ({})", documented.len())?;
    if documented.is_empty() {
        writeln!(
            out,
            "\n_No patch entries. Add `~/.cordial/{{project}}/patches/amenable.json` to document intentional exclusions._\n"
        )?;
    } else {
        writeln!(out)?;
        for entry in documented {
            let reason = entry
                .skip_reason
                .as_deref()
                .or_else(|| skip_map.get(&entry.type_path).map(|e| e.reason.as_str()))
                .unwrap_or("documented in patch set");
            if entry.status == AmenableStdStatus::Skipped {
                writeln!(out, "- `{}` — {}", entry.type_path, reason)?;
            } else {
                writeln!(
                    out,
                    "- `{}` ({}, scoped exception) — {}",
                    entry.type_path, entry.status, reason
                )?;
            }
        }
        writeln!(out)?;
    }

    Ok(out)
}

#[instrument(level = "debug", skip(report))]
pub fn render_amenable_std_summary_md(report: &AmenableStdReport) -> String {
    let accountable = report.entries.len().saturating_sub(report.skipped_count);
    format!(
        "# Amenable std registry coverage summary\n\n\
        **Profile:** std type list vs `RustStdStandard<T>` evidence + verifier witnesses  \n\
        **Scope:** {scope}  \n\
        **Impl crate:** `{impl_crate}`  \n\
        **Source inventory:** `{source}` (std + core + alloc)  \n\
        **Total types:** {total}  \n\
        **Accountable:** {accountable}  \n\
        **Complete:** {complete} ({pct:.1}%)  \n\
        **Partial:** {partial}  \n\
        **Missing evidence:** {missing}  \n\
        **Skipped:** {skipped}\n\n\
        Open `std.checklist.md` for the actionable gap list.\n",
        scope = if report.include_nightly {
            "stable + nightly std types"
        } else {
            "stable std types only"
        },
        impl_crate = report.impl_crate,
        source = report.source_crate,
        total = report.entries.len(),
        accountable = accountable,
        complete = report.complete_count,
        pct = report.coverage_pct(),
        partial = report.partial_count,
        missing = report.missing_count,
        skipped = report.skipped_count,
    )
}
