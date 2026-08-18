//! Shadow coverage gap classification and consolidated gap reports.

use tracing::instrument;

use super::types::{ShadowGapEntry, ShadowGapKind, ShadowReport, ShadowRow, ShadowStatus};
use super::verification::{ShadowImplStatus, shadow_verification_gap};

const INFRA_SUFFIXES: &[&str] = &[
    "Params",
    "ParamsStyle",
    "Plugin",
    "Ctx",
    "Descriptor",
    "Factory",
    "Hook",
    "Json",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShadowCoverageKind {
    Covered,
    Missing,
    Drifted,
    PossiblyStale,
    InfrastructureExtra,
}

#[instrument(level = "trace")]
pub fn is_shadow_infrastructure_name(bare_name: &str) -> bool {
    INFRA_SUFFIXES
        .iter()
        .any(|suffix| bare_name.ends_with(suffix))
}

/// Build the consolidated shadow gaps list from multiple per-pair reports.
#[instrument(level = "debug")]
pub fn build_shadow_gaps(pairs: &[(&str, &str, &ShadowReport)]) -> Vec<ShadowGapEntry> {
    let mut entries = Vec::new();

    for (target_crate, shadow_crate, report) in pairs {
        for row in &report.rows {
            let assessment = assess_shadow_row(row);
            if row.status == ShadowStatus::Covered {
                continue;
            }
            let Some(gap_kind) = assessment.primary_gap_kind else {
                continue;
            };
            entries.push(ShadowGapEntry {
                target_crate: (*target_crate).to_string(),
                shadow_crate: (*shadow_crate).to_string(),
                item_path: row.item_path.clone(),
                item_kind: row.item_kind.as_str().to_string(),
                gap_kind,
                matched_shadow_item: row.shadow_item.clone(),
                drift_confidence: row.drift_confidence.clone(),
                shadow_elicit_impl: row.shadow_elicit_impl.clone(),
                shadow_can_be_direct: row.shadow_can_be_direct.clone(),
                shadow_missing_external_traits: row.shadow_missing_external_traits.clone(),
                shadow_missing_our_traits: row.shadow_missing_our_traits.clone(),
                action: assessment.action,
                notes: row.notes.clone(),
            });
        }

        for row in &report.rows {
            if !shadow_verification_gap(row) {
                continue;
            }
            entries.push(ShadowGapEntry {
                target_crate: (*target_crate).to_string(),
                shadow_crate: (*shadow_crate).to_string(),
                item_path: row.item_path.clone(),
                item_kind: row.item_kind.as_str().to_string(),
                gap_kind: ShadowGapKind::ShadowVerificationGap,
                matched_shadow_item: row.shadow_item.clone(),
                drift_confidence: row.drift_confidence.clone(),
                shadow_elicit_impl: row.shadow_elicit_impl.clone(),
                shadow_can_be_direct: row.shadow_can_be_direct.clone(),
                shadow_missing_external_traits: row.shadow_missing_external_traits.clone(),
                shadow_missing_our_traits: row.shadow_missing_our_traits.clone(),
                action: build_shadow_verification_action(row),
                notes: row.notes.clone(),
            });
        }
    }

    entries.sort_by(|left, right| {
        shadow_gap_order(&left.gap_kind)
            .cmp(&shadow_gap_order(&right.gap_kind))
            .then(left.target_crate.cmp(&right.target_crate))
            .then(left.item_path.cmp(&right.item_path))
    });

    entries
}

struct ShadowRowAssessment {
    primary_gap_kind: Option<ShadowGapKind>,
    action: String,
}

fn assess_shadow_row(row: &ShadowRow) -> ShadowRowAssessment {
    let coverage_kind = classify_shadow_coverage_kind(row);
    let primary_gap_kind = primary_gap_for_coverage(&coverage_kind);
    let mut action = shadow_coverage_action(row, &coverage_kind);
    if shadow_verification_gap(row) {
        let verification_action = build_shadow_verification_action(row);
        if action.is_empty() {
            action = verification_action;
        } else {
            action.push_str("; then ");
            action.push_str(&verification_action);
        }
    }
    ShadowRowAssessment {
        primary_gap_kind,
        action,
    }
}

fn classify_shadow_coverage_kind(row: &ShadowRow) -> ShadowCoverageKind {
    match row.status {
        ShadowStatus::Covered => ShadowCoverageKind::Covered,
        ShadowStatus::Missing => ShadowCoverageKind::Missing,
        ShadowStatus::Drifted => ShadowCoverageKind::Drifted,
        ShadowStatus::Extra => {
            let bare = row.item_path.rsplit("::").next().unwrap_or(&row.item_path);
            if is_shadow_infrastructure_name(bare) {
                ShadowCoverageKind::InfrastructureExtra
            } else {
                ShadowCoverageKind::PossiblyStale
            }
        }
    }
}

fn primary_gap_for_coverage(coverage_kind: &ShadowCoverageKind) -> Option<ShadowGapKind> {
    match coverage_kind {
        ShadowCoverageKind::Covered => None,
        ShadowCoverageKind::Missing => Some(ShadowGapKind::Missing),
        ShadowCoverageKind::Drifted => Some(ShadowGapKind::Drifted),
        ShadowCoverageKind::PossiblyStale => Some(ShadowGapKind::PossiblyStale),
        ShadowCoverageKind::InfrastructureExtra => Some(ShadowGapKind::InfrastructureExtra),
    }
}

fn shadow_coverage_action(row: &ShadowRow, coverage_kind: &ShadowCoverageKind) -> String {
    match coverage_kind {
        ShadowCoverageKind::Covered => String::new(),
        ShadowCoverageKind::Missing => {
            if row.item_kind.is_type() {
                format!(
                    "Add a shadow for upstream `{}` and make the new wrapper `ElicitComplete`",
                    row.item_path
                )
            } else {
                format!(
                    "Add a shadow item for upstream `{}` so the full public API surface is represented",
                    row.item_path
                )
            }
        }
        ShadowCoverageKind::Drifted => format!(
            "Rename or replace `{}` so upstream `{}` is shadowed exactly",
            row.shadow_item, row.item_path
        ),
        ShadowCoverageKind::PossiblyStale => format!(
            "Audit `{}`: remove it if stale, or rename/remap it to an upstream public item",
            row.item_path
        ),
        ShadowCoverageKind::InfrastructureExtra => {
            "Shadow-only infrastructure item; keep unless it should instead map to an upstream API item"
                .to_string()
        }
    }
}

fn shadow_gap_order(kind: &ShadowGapKind) -> u8 {
    match kind {
        ShadowGapKind::Missing => 0,
        ShadowGapKind::Drifted => 1,
        ShadowGapKind::PossiblyStale => 2,
        ShadowGapKind::InfrastructureExtra => 3,
        ShadowGapKind::ShadowVerificationGap => 4,
    }
}

fn build_shadow_verification_action(row: &ShadowRow) -> String {
    let missing_our = row.shadow_missing_our_traits.as_str();
    let missing_external = row.shadow_missing_external_traits.as_str();
    let can_be_direct = row.shadow_can_be_direct == "true";

    if !missing_our.is_empty() && can_be_direct {
        format!(
            "Finish `{}` by adding our traits: {}; then add `impl ElicitComplete`",
            row.shadow_item,
            missing_our.replace(';', ", ")
        )
    } else if !missing_our.is_empty() {
        format!(
            "Finish `{}` by adding our traits: {}; also add external traits: {}",
            row.shadow_item,
            missing_our.replace(';', ", "),
            missing_external.replace(';', ", ")
        )
    } else if can_be_direct {
        format!("Add `impl ElicitComplete for {} {{}}`", row.shadow_item)
    } else {
        format!(
            "Add external traits to `{}` so it can support `ElicitComplete`: {}",
            row.shadow_item,
            missing_external.replace(';', ", ")
        )
    }
}

/// Row assessment fields used when rendering per-pair shadow CSV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowRowRender {
    pub coverage_kind: String,
    pub primary_gap_kind: String,
    pub verification_gap: bool,
    pub verification_ready: bool,
    pub action: String,
}

#[instrument(level = "debug")]
pub fn render_shadow_row(row: &ShadowRow) -> ShadowRowRender {
    let assessment = assess_shadow_row(row);
    let coverage_kind = match classify_shadow_coverage_kind(row) {
        ShadowCoverageKind::Covered => "Covered",
        ShadowCoverageKind::Missing => "Missing",
        ShadowCoverageKind::Drifted => "Drifted",
        ShadowCoverageKind::PossiblyStale => "PossiblyStale",
        ShadowCoverageKind::InfrastructureExtra => "InfrastructureExtra",
    };
    let verification_ready = row.shadow_elicit_impl == ShadowImplStatus::Complete.as_str()
        || row.shadow_elicit_impl == ShadowImplStatus::CompleteFactory.as_str();
    ShadowRowRender {
        coverage_kind: coverage_kind.to_string(),
        primary_gap_kind: assessment
            .primary_gap_kind
            .map(ShadowGapKind::as_str)
            .unwrap_or("")
            .to_string(),
        verification_gap: shadow_verification_gap(row),
        verification_ready,
        action: assessment.action,
    }
}

#[instrument(level = "debug", skip(path))]
pub fn api_family(path: &str) -> String {
    let mut parts = path.split("::");
    let mut family = Vec::with_capacity(3);
    for _ in 0..3 {
        if let Some(part) = parts.next() {
            family.push(part);
        } else {
            break;
        }
    }
    family.join("::")
}
