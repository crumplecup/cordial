//! Per-target summary of elicitation-core and shadow support for upstream types.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::CordialResult;
use crate::ir::WorkspaceIr;
use crate::objects::{Disposition, Finding, MapFindingSink};
use crate::plugin::{
    ELICITATION_INTERFACE_SHADOW_CRATES, active_tracked_targets, compare_tracked_target_roster,
    discover_active_shadow_pairs, is_interface_shadow_crate, tracked_target_for_upstream,
};
use crate::session::{RunAll, RunFilter, SessionView};
use crate::targets::discover_crate_targets;

/// Roster comparison for workspace members vs the elicitation tracked-target roster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrackedTargetRosterDigest {
    pub workspace_elicit_members: Vec<String>,
    pub interface_crates: Vec<String>,
    pub gaps: TrackedTargetRosterGapRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrackedTargetRosterGapRecord {
    pub members_without_tracked_target: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShadowCoreSupportSummary {
    pub target_crate: String,
    pub shadow_crate: String,
    pub elicitation_impl: bool,
    pub target_types: usize,
    pub target_inventory_available: bool,
    pub impl_report_available: bool,
    pub our_traits_done: usize,
    pub missing_our_traits: usize,
    pub direct_elicit_complete: usize,
    pub wrapper_covered_types: usize,
    pub coverage_pct: f64,
    pub status: ShadowCoreSupportStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShadowCoreSupportStatus {
    CoreTracked,
    CorePending,
    ShadowOnly,
    Missing,
}

impl ShadowCoreSupportStatus {
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CoreTracked => "CoreTracked",
            Self::CorePending => "CorePending",
            Self::ShadowOnly => "ShadowOnly",
            Self::Missing => "Missing",
        }
    }
}

impl std::fmt::Display for ShadowCoreSupportStatus {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShadowCoreSupportDigest {
    pub roster: TrackedTargetRosterDigest,
    pub summaries: Vec<ShadowCoreSupportSummary>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ImplCrateRollup {
    pub types: usize,
    pub our_traits_done: usize,
    pub direct_elicit_complete: usize,
    pub wrapper_covered_types: usize,
}

/// Build the combined core + shadow support digest for the current workspace run.
#[instrument(
    level = "debug",
    skip(session, _filter, findings, workspace),
    err(level = "warn")
)]
pub fn build_shadow_core_support_digest(
    session: &dyn SessionView,
    _filter: &dyn RunFilter,
    findings: &[&dyn Finding],
    workspace: &WorkspaceIr,
) -> CordialResult<ShadowCoreSupportDigest> {
    let members: Vec<String> = discover_crate_targets(session.project_root(), &RunAll)?
        .into_iter()
        .map(|target| target.crate_name)
        .collect();
    let member_set: HashSet<String> = members.iter().cloned().collect();
    let impl_rollups = rollup_impl_findings(findings);

    let mut summaries = Vec::new();
    for target in active_tracked_targets(&member_set) {
        summaries.push(build_shadow_core_support_summary(
            target.upstream,
            target.shadow,
            target.elicitation_impl,
            workspace.rustdoc_inventory_type_count(target.upstream),
            workspace.crate_ir(target.upstream).is_some(),
            impl_rollups.get(target.upstream),
        )?);
    }

    if summaries.is_empty() {
        for pair in discover_active_shadow_pairs(session.project_root(), &RunAll)? {
            if active_tracked_targets(&member_set)
                .into_iter()
                .any(|entry| entry.upstream == pair.upstream)
            {
                continue;
            }
            let elicitation_impl = tracked_target_for_upstream(&pair.upstream)
                .map(|entry| entry.elicitation_impl)
                .unwrap_or(false);
            summaries.push(build_shadow_core_support_summary(
                &pair.upstream,
                &pair.shadow,
                elicitation_impl,
                workspace.rustdoc_inventory_type_count(&pair.upstream),
                workspace.crate_ir(&pair.upstream).is_some(),
                impl_rollups.get(pair.upstream.as_str()),
            )?);
        }
    }

    summaries.sort_by(|left, right| left.target_crate.cmp(&right.target_crate));

    Ok(ShadowCoreSupportDigest {
        roster: build_tracked_target_roster_digest(&members),
        summaries,
    })
}

#[instrument(level = "debug", skip(impl_rollup), err(level = "warn"))]
pub fn build_shadow_core_support_summary(
    upstream: &str,
    shadow: &str,
    elicitation_impl: bool,
    target_types: usize,
    target_inventory_available: bool,
    impl_rollup: Option<&ImplCrateRollup>,
) -> CordialResult<ShadowCoreSupportSummary> {
    if let Some(rollup) = impl_rollup.filter(|rollup| rollup.types > 0) {
        let coverage_pct = percent(rollup.our_traits_done, rollup.types);
        return Ok(ShadowCoreSupportSummary {
            target_crate: upstream.to_string(),
            shadow_crate: shadow.to_string(),
            elicitation_impl,
            target_types: rollup.types,
            target_inventory_available,
            impl_report_available: true,
            our_traits_done: rollup.our_traits_done,
            missing_our_traits: rollup.types.saturating_sub(rollup.our_traits_done),
            direct_elicit_complete: rollup.direct_elicit_complete,
            wrapper_covered_types: rollup.wrapper_covered_types,
            coverage_pct,
            status: ShadowCoreSupportStatus::CoreTracked,
        });
    }

    let wrapper_covered_types = 0usize;

    let status = if !target_inventory_available {
        ShadowCoreSupportStatus::Missing
    } else if elicitation_impl {
        ShadowCoreSupportStatus::CorePending
    } else {
        ShadowCoreSupportStatus::ShadowOnly
    };

    Ok(ShadowCoreSupportSummary {
        target_crate: upstream.to_string(),
        shadow_crate: shadow.to_string(),
        elicitation_impl,
        target_types,
        target_inventory_available,
        impl_report_available: false,
        our_traits_done: 0,
        missing_our_traits: target_types,
        direct_elicit_complete: 0,
        wrapper_covered_types,
        coverage_pct: if target_types == 0 || elicitation_impl {
            0.0
        } else {
            percent(wrapper_covered_types, target_types)
        },
        status,
    })
}

#[instrument(level = "debug")]
pub fn build_tracked_target_roster_digest(
    workspace_members: &[String],
) -> TrackedTargetRosterDigest {
    let gaps = compare_tracked_target_roster(workspace_members);
    TrackedTargetRosterDigest {
        workspace_elicit_members: workspace_members
            .iter()
            .filter(|member| member.starts_with("elicit_") || *member == "elicitation")
            .cloned()
            .collect(),
        interface_crates: workspace_members
            .iter()
            .filter(|member| is_interface_shadow_crate(member))
            .cloned()
            .collect(),
        gaps: TrackedTargetRosterGapRecord {
            members_without_tracked_target: gaps.members_without_tracked_target,
        },
    }
}

#[instrument(level = "debug", skip(findings))]
fn rollup_impl_findings(findings: &[&dyn Finding]) -> BTreeMap<String, ImplCrateRollup> {
    let mut by_crate: BTreeMap<String, ImplCrateRollup> = BTreeMap::new();
    for finding in findings {
        if finding.rule().category() != "impl-coverage" {
            continue;
        }
        let row = finding_row(*finding);
        let crate_name = row.get("crate").cloned().unwrap_or_else(|| "?".to_string());
        let rollup = by_crate.entry(crate_name).or_default();
        rollup.types += 1;

        let missing_our = row
            .get("missing_our_traits")
            .is_some_and(|value| !value.is_empty());
        if !missing_our {
            rollup.our_traits_done += 1;
        }

        let direct_complete = finding.disposition() == Disposition::Exemplar
            && row.get("gap_kind").is_none_or(String::is_empty)
            && !truthy(row.get("covered_indirectly"));
        if direct_complete {
            rollup.direct_elicit_complete += 1;
        }

        if truthy(row.get("covered_indirectly"))
            || row
                .get("coverage_provider")
                .is_some_and(|value| !value.is_empty())
        {
            rollup.wrapper_covered_types += 1;
        }
    }
    by_crate
}

/// Summary section appended to elicitation `summary.md` (elicit_doc parity).
#[instrument(level = "debug", skip(digest))]
pub fn render_shadow_core_support_summary_section(digest: &ShadowCoreSupportDigest) -> String {
    let mut out = String::new();
    out.push_str("## Target Support (core + shadow)\n\n");
    out.push_str(
        "Each row is one upstream crate from the elicitation tracked-target roster: core impl coverage and the matching `elicit_*` shadow mirror share the same target version.\n\n",
    );

    if !digest.roster.gaps.members_without_tracked_target.is_empty() {
        out.push_str("### Workspace members not in tracked-target roster\n\n");
        out.push_str(&format!(
            "{}\n\n",
            format_crate_list(&digest.roster.gaps.members_without_tracked_target)
        ));
    }

    let core_pending: Vec<_> = digest
        .summaries
        .iter()
        .filter(|summary| summary.status == ShadowCoreSupportStatus::CorePending)
        .collect();
    if !core_pending.is_empty() {
        out.push_str("### Awaiting impl coverage reports\n\n");
        out.push_str(
            "These targets have `elicitation_impl: true` and inventories but no impl report yet.\n\n",
        );
        for summary in core_pending {
            out.push_str(&format!(
                "- `{}` ↔ `{}` ({} types inventoried)\n",
                summary.target_crate, summary.shadow_crate, summary.target_types
            ));
        }
        out.push('\n');
    }

    let shadow_only: Vec<_> = digest
        .summaries
        .iter()
        .filter(|summary| summary.status == ShadowCoreSupportStatus::ShadowOnly)
        .collect();
    if !shadow_only.is_empty() {
        out.push_str("### Shadow-only targets (not wired in elicitation)\n\n");
        for summary in shadow_only {
            out.push_str(&format!(
                "- `{}` ↔ `{}` ({} types via shadow inventory)\n",
                summary.target_crate, summary.shadow_crate, summary.target_types
            ));
        }
        out.push('\n');
    }

    out.push_str("### Per upstream target\n\n");
    out.push_str(
        "| Upstream | Shadow | Core impl | Types | Status | OurTraitsDone | Wrapper-covered | Core coverage |\n",
    );
    out.push_str(
        "|----------|--------|-----------|------:|--------|--------------:|----------------:|--------------:|\n",
    );

    for summary in &digest.summaries {
        let our_traits = if summary.impl_report_available {
            summary.our_traits_done.to_string()
        } else {
            "—".to_string()
        };
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {} | {} | {:.1}% |\n",
            summary.target_crate,
            summary.shadow_crate,
            yes_no(summary.elicitation_impl),
            summary.target_types,
            summary.status,
            our_traits,
            summary.wrapper_covered_types,
            summary.coverage_pct,
        ));
    }
    out.push('\n');
    out
}

#[instrument(level = "debug", skip(roster))]
pub fn render_tracked_target_roster_markdown(roster: &TrackedTargetRosterDigest) -> String {
    let mut out = String::new();
    out.push_str("# Tracked target roster\n\n");
    out.push_str(
        "_Each entry in the elicitation tracked-target roster drives shadow mirror coverage; `elicitation_impl` controls whether impl-dep core metrics run._\n\n",
    );
    out.push_str("Excluded interface crates: ");
    out.push_str(
        &ELICITATION_INTERFACE_SHADOW_CRATES
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str("\n\n");

    out.push_str("## Workspace `elicit_*` members\n\n");
    if roster.workspace_elicit_members.is_empty() {
        out.push_str("_None discovered._\n\n");
    } else {
        let without_target: HashSet<&str> = roster
            .gaps
            .members_without_tracked_target
            .iter()
            .map(String::as_str)
            .collect();
        for member in &roster.workspace_elicit_members {
            let suffix = if is_interface_shadow_crate(member) {
                " _(interface)_"
            } else if without_target.contains(member.as_str()) {
                " _(not in tracked-target roster)_"
            } else {
                ""
            };
            out.push_str(&format!("- `{member}`{suffix}\n"));
        }
        out.push('\n');
    }

    write_gap_list(
        &mut out,
        "## Workspace members not in tracked-target roster",
        &roster.gaps.members_without_tracked_target,
        "Add a tracked target entry or classify the member as interface.",
    );
    out
}

#[instrument(level = "info", skip(items))]
fn write_gap_list(out: &mut String, heading: &str, items: &[String], action: &str) {
    out.push_str(heading);
    out.push_str("\n\n");
    if items.is_empty() {
        out.push_str("_None — workspace mirrors match the configured target list._\n\n");
        return;
    }
    for item in items {
        out.push_str(&format!("- `{item}`\n"));
    }
    out.push_str(&format!("\n_{action}_\n\n"));
}

#[instrument(level = "debug")]
fn format_crate_list(crates: &[String]) -> String {
    if crates.is_empty() {
        "—".to_string()
    } else {
        crates
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
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
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64 * 100.0
    }
}

#[instrument(level = "debug")]
fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
