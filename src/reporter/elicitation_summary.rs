//! Elicitation coverage summary sections — metric parity with elicit_doc `summary.md`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::error::CordialResult;
use crate::ir::WorkspaceIr;
use crate::objects::{Artifact, Disposition, Finding, MapFindingSink, TextArtifact};
use crate::session::{RunFilter, SessionView};

use tracing::instrument;
/// Rollup body plus optional coverage-plugin artifacts.
pub struct ElicitationCoverageRollup {
    pub body: String,
    pub extra_artifacts: Vec<Box<dyn Artifact>>,
}

/// Build the elicitation plugin body and shadow-core-support artifact for the workspace rollup.
#[instrument(
    level = "debug",
    skip(session, filter, findings, workspace),
    err(level = "warn")
)]
pub fn build_elicitation_coverage_rollup(
    session: &dyn SessionView,
    filter: &dyn RunFilter,
    findings: &[&dyn Finding],
    workspace: &WorkspaceIr,
) -> CordialResult<ElicitationCoverageRollup> {
    let mut out = String::new();
    write_impl_coverage_section(workspace, findings, &mut out)?;
    write_impl_gaps_section(findings, &mut out)?;
    write_shadow_coverage_section(workspace, findings, &mut out)?;
    let digest =
        crate::digest::build_shadow_core_support_digest(session, filter, findings, workspace)?;
    out.push_str(&crate::digest::render_shadow_core_support_summary_section(
        &digest,
    ));
    let extra_artifacts = vec![Box::new(TextArtifact {
        name: "shadow-core-support.json".to_string(),
        media_type: "application/json".to_string(),
        body: serde_json::to_string_pretty(&digest)?,
    }) as Box<dyn Artifact>];
    Ok(ElicitationCoverageRollup {
        body: out,
        extra_artifacts,
    })
}

#[instrument(level = "info", skip(workspace, findings), err(level = "warn"))]
fn write_impl_coverage_section(
    workspace: &WorkspaceIr,
    findings: &[&dyn Finding],
    out: &mut String,
) -> CordialResult<()> {
    let mut by_crate: BTreeMap<String, ImplCrateMetrics> = BTreeMap::new();
    for finding in findings {
        if finding.rule().category() != "impl-coverage" {
            continue;
        }
        let row = finding_row(*finding);
        let crate_name = row.get("crate").cloned().unwrap_or_else(|| "?".to_string());
        let metrics = by_crate.entry(crate_name).or_default();
        metrics.types += 1;

        let missing_our = row
            .get("missing_our_traits")
            .is_some_and(|value| !value.is_empty());
        if !missing_our {
            metrics.our_traits_done += 1;
        } else {
            metrics.missing_our_traits += 1;
        }

        let direct_complete = finding.disposition() == Disposition::Exemplar
            && row.get("gap_kind").is_none_or(String::is_empty)
            && !truthy(row.get("covered_indirectly"));
        if direct_complete {
            metrics.elicit_complete += 1;
        }

        if row
            .get("gap_kind")
            .is_some_and(|kind| kind == "ReadyForElicitComplete")
            || truthy(row.get("elicit_complete_gap"))
        {
            metrics.elicit_complete_gap += 1;
        }

        if finding.disposition() == Disposition::Suppressed {
            metrics.externally_blocked += 1;
        }
    }

    out.push_str("## Impl Coverage\n\n");
    if by_crate.is_empty() {
        out.push_str("_No impl coverage findings._\n\n");
        return Ok(());
    }

    out.push_str("| Crate | Version | Types | OurTraitsDone | MissingOurTraits | ElicitComplete | ElicitCompleteGap | ExternallyBlocked | Coverage |\n");
    out.push_str("|-------|---------|------:|--------------:|-----------------:|---------------:|------------------:|------------------:|---------:|\n");

    let mut totals = ImplCrateMetrics::default();
    for (crate_name, metrics) in &by_crate {
        let version = lookup_crate_version(workspace, crate_name);
        let pct = percent(metrics.our_traits_done, metrics.types);
        writeln!(
            out,
            "| `{crate_name}` | {version} | {} | {} | {} | {} | {} | {} | {pct:.1}% |",
            metrics.types,
            metrics.our_traits_done,
            metrics.missing_our_traits,
            metrics.elicit_complete,
            metrics.elicit_complete_gap,
            metrics.externally_blocked,
        )?;
        totals.merge(metrics);
    }

    let total_pct = percent(totals.our_traits_done, totals.types);
    writeln!(
        out,
        "| **Total** | | **{}** | **{}** | **{}** | **{}** | **{}** | **{}** | **{total_pct:.1}%** |",
        totals.types,
        totals.our_traits_done,
        totals.missing_our_traits,
        totals.elicit_complete,
        totals.elicit_complete_gap,
        totals.externally_blocked,
    )?;
    out.push_str("\n`OurTraitsDone` counts effective trait coverage. A trait counts when it is satisfied either directly on the target type or indirectly via a wrapper that deductively covers that target. Lifetime-bound types such as `Pixels<'a, R>` are still not expected to implement `Elicitation` or `ElicitIntrospect` directly because `Elicitation` requires `'static`.\n\n");
    out.push_str("`Coverage` uses that same effective-coverage rule. A type counts as covered when every elicitation-owned trait that should exist for that target is present, either directly or through wrapper coverage, even if direct `ElicitComplete` is blocked by lifetimes or the orphan rule.\n\n");
    out.push_str("`ExternallyBlocked` counts true orphan-rule blockers only: the implementable elicitation-owned traits are present, but direct `ElicitComplete` is blocked by missing `Serialize`, `Deserialize`, or `JsonSchema` on the target type. Lifetime-bound rows still count toward `Coverage` when every implementable elicitation-owned trait is present, but they are not counted as external blockers.\n\n");
    out.push_str("---\n\n");
    Ok(())
}

#[instrument(level = "info", skip(findings), err(level = "warn"))]
fn write_impl_gaps_section(findings: &[&dyn Finding], out: &mut String) -> CordialResult<()> {
    let mut missing_our = 0usize;
    let mut ready = 0usize;
    let mut gated = 0usize;

    for finding in findings {
        if finding.rule().category() != "impl-coverage"
            || finding.disposition() != Disposition::Open
        {
            continue;
        }
        match finding_row(*finding).get("gap_kind").map(String::as_str) {
            Some("MissingOurTraits") => missing_our += 1,
            Some("ReadyForElicitComplete") => ready += 1,
            Some("FeatureGatedExternal") => gated += 1,
            _ => {}
        }
    }

    let total = missing_our + ready + gated;
    if total == 0 {
        return Ok(());
    }

    out.push_str("### Impl Gaps\n\n");
    out.push_str("| Kind | Count | Notes |\n");
    out.push_str("|------|------:|-------|\n");
    writeln!(
        out,
        "| MissingOurTraits | {missing_our} | Missing one or more elicitation-owned support traits |"
    )?;
    writeln!(
        out,
        "| ReadyForElicitComplete | {ready} | All prerequisites present; only `impl ElicitComplete` is missing |"
    )?;
    writeln!(
        out,
        "| FeatureGatedExternal | {gated} | Missing external serde/schemars traits may be unlockable with more features |"
    )?;
    writeln!(out, "| **Total** | **{total}** | |")?;
    out.push_str("\n---\n\n");
    Ok(())
}

#[instrument(level = "info", skip(workspace, findings), err(level = "warn"))]
fn write_shadow_coverage_section(
    workspace: &WorkspaceIr,
    findings: &[&dyn Finding],
    out: &mut String,
) -> CordialResult<()> {
    let mut pairs: BTreeMap<(String, String), ShadowPairMetrics> = BTreeMap::new();
    for finding in findings {
        if finding.rule().category() != "shadow-pair" {
            continue;
        }
        let row = finding_row(*finding);
        let target = row
            .get("target_crate")
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        let shadow = row
            .get("shadow_crate")
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        let metrics = pairs.entry((target.clone(), shadow.clone())).or_default();
        if metrics.coverage_pct.is_none() {
            metrics.coverage_pct = row.get("coverage_pct").cloned();
        }

        match row.get("status").map(String::as_str) {
            Some("Covered") => metrics.covered += 1,
            Some("Drifted") => metrics.drifted += 1,
            Some("Missing") => metrics.missing += 1,
            _ => {}
        }
        if truthy(row.get("verification_gap")) {
            metrics.verification_gaps += 1;
        }

        match row.get("primary_gap_kind").map(String::as_str) {
            Some("Missing") => metrics.gap_missing += 1,
            Some("Drifted") => metrics.gap_drifted += 1,
            Some("PossiblyStale") => metrics.gap_stale += 1,
            Some("InfrastructureExtra") => metrics.gap_infra += 1,
            _ => {}
        }
        if truthy(row.get("verification_gap"))
            && row.get("primary_gap_kind").is_none_or(String::is_empty)
        {
            metrics.gap_verification += 1;
        }
    }

    if pairs.is_empty() {
        return Ok(());
    }

    out.push_str("## Shadow Coverage\n\n");
    out.push_str("| Upstream | Version | Shadow Crate | Covered | Drifted | Total | VerificationGaps | Coverage |\n");
    out.push_str("|----------|---------|-------------|--------:|--------:|------:|-----------------:|---------:|\n");

    for ((upstream, shadow), metrics) in &pairs {
        let version = lookup_crate_version(workspace, upstream);
        let total = metrics.covered + metrics.drifted + metrics.missing;
        let pct = metrics
            .coverage_pct
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or_else(|| percent_f64(metrics.covered + metrics.drifted, total));
        writeln!(
            out,
            "| `{upstream}` | {version} | `{shadow}` | {} | {} | {total} | {} | {pct:.1}% |",
            metrics.covered, metrics.drifted, metrics.verification_gaps,
        )?;
    }
    out.push('\n');

    let gap_total = pairs
        .values()
        .map(|metrics| {
            metrics.gap_missing
                + metrics.gap_drifted
                + metrics.gap_stale
                + metrics.gap_infra
                + metrics.gap_verification
        })
        .sum::<usize>();
    if gap_total > 0 {
        let totals = pairs
            .values()
            .fold(ShadowPairMetrics::default(), |mut acc, row| {
                acc.gap_missing += row.gap_missing;
                acc.gap_drifted += row.gap_drifted;
                acc.gap_stale += row.gap_stale;
                acc.gap_infra += row.gap_infra;
                acc.gap_verification += row.gap_verification;
                acc
            });
        out.push_str("### Shadow Gaps\n\n");
        out.push_str("| Kind | Count | Notes |\n");
        out.push_str("|------|------:|-------|\n");
        writeln!(
            out,
            "| Missing | {} | Upstream public item not yet shadowed |",
            totals.gap_missing
        )?;
        writeln!(
            out,
            "| Drifted | {} | Probable rename or naming drift in the shadow crate |",
            totals.gap_drifted
        )?;
        writeln!(
            out,
            "| PossiblyStale | {} | Shadow item with no matching upstream — needs audit |",
            totals.gap_stale
        )?;
        writeln!(
            out,
            "| InfrastructureExtra | {} | Shadow-only infrastructure item — expected |",
            totals.gap_infra
        )?;
        writeln!(
            out,
            "| ShadowVerificationGap | {} | Matched shadow type exists but is not yet `ElicitComplete`-ready |",
            totals.gap_verification
        )?;
        writeln!(out, "| **Total** | **{gap_total}** | |")?;
        out.push('\n');
    }

    Ok(())
}

#[derive(Debug, Default, Clone, Copy)]
struct ImplCrateMetrics {
    types: usize,
    our_traits_done: usize,
    missing_our_traits: usize,
    elicit_complete: usize,
    elicit_complete_gap: usize,
    externally_blocked: usize,
}

impl ImplCrateMetrics {
    #[instrument(level = "debug", skip(self, other))]
    fn merge(&mut self, other: &Self) {
        self.types += other.types;
        self.our_traits_done += other.our_traits_done;
        self.missing_our_traits += other.missing_our_traits;
        self.elicit_complete += other.elicit_complete;
        self.elicit_complete_gap += other.elicit_complete_gap;
        self.externally_blocked += other.externally_blocked;
    }
}

#[derive(Debug, Default, Clone)]
struct ShadowPairMetrics {
    covered: usize,
    drifted: usize,
    missing: usize,
    verification_gaps: usize,
    coverage_pct: Option<String>,
    gap_missing: usize,
    gap_drifted: usize,
    gap_stale: usize,
    gap_infra: usize,
    gap_verification: usize,
}

#[instrument(level = "debug", skip(workspace))]
fn lookup_crate_version(workspace: &WorkspaceIr, crate_name: &str) -> String {
    workspace
        .crate_version(crate_name)
        .unwrap_or_else(|| "unknown".to_string())
}

#[instrument(level = "debug", skip(finding))]
fn finding_row(finding: &dyn Finding) -> BTreeMap<String, String> {
    let mut sink = MapFindingSink::default();
    finding.emit(&mut sink);
    sink.fields.into_iter().collect()
}

#[instrument(level = "debug")]
fn truthy(value: Option<&String>) -> bool {
    matches!(value.map(String::as_str), Some("true" | "1" | "yes"))
}

#[instrument(level = "debug")]
fn percent(numerator: usize, denominator: usize) -> f64 {
    percent_f64(numerator, denominator)
}

#[instrument(level = "debug")]
fn percent_f64(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}
