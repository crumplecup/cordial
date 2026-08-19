use crate::error::CordialResult;
use crate::hooks::WorkspaceAssessor;
use crate::ir::WorkspaceIr;
use crate::objects::{Disposition, Finding, NodeAnchor};
use crate::plugin::discover_active_shadow_pairs;
use crate::session::{RunFilter, SessionView};
use crate::shadow::{
    ShadowStatus, build_shadow_gaps, build_shadow_pair_report_from_workspace,
    render_shadow_method_checklist,
};

use super::types::{
    CrossCrateShadowFinding, ShadowMethodChecklistFinding, ShadowPairChecklistRule, ShadowPairRule,
};

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
pub struct CrossCrateShadowWorkspaceAssessor;

impl CrossCrateShadowWorkspaceAssessor {
    pub const ID: &'static str = "cross-crate-shadow-workspace-assessor";
}

impl WorkspaceAssessor for CrossCrateShadowWorkspaceAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, workspace, session, filter))]
    fn assess(
        &self,
        workspace: &WorkspaceIr,
        session: &dyn SessionView,
        filter: &dyn RunFilter,
    ) -> CordialResult<Vec<Box<dyn Finding>>> {
        let pairs = discover_active_shadow_pairs(session.project_root(), filter)?;
        let mut findings = Vec::new();

        for pair in pairs {
            let report =
                build_shadow_pair_report_from_workspace(workspace, &pair.upstream, &pair.shadow)?;
            let anchor = workspace
                .crate_ir(&pair.upstream)
                .map(|ir| NodeAnchor(ir.root))
                .unwrap_or(NodeAnchor(crate::ir::NodeId(0)));
            findings.extend(findings_from_shadow_pair_report(
                &report,
                &pair.upstream,
                &pair.shadow,
                anchor,
            )?);
        }
        Ok(findings)
    }
}

#[instrument(level = "debug", skip(report, anchor), err(level = "warn"))]
pub fn findings_from_shadow_pair_report(
    report: &crate::shadow::ShadowReport,
    upstream: &str,
    shadow: &str,
    anchor: NodeAnchor,
) -> CordialResult<Vec<Box<dyn Finding>>> {
    let pair_refs = [(upstream, shadow, report)];
    let gaps = build_shadow_gaps(&pair_refs);
    let gap_paths: std::collections::HashSet<String> = gaps
        .iter()
        .filter(|entry| entry.gap_kind != crate::shadow::ShadowGapKind::ShadowVerificationGap)
        .map(|entry| entry.item_path.clone())
        .collect();

    let mut findings: Vec<Box<dyn Finding>> = report
        .rows
        .iter()
        .map(|row| {
            let disposition = if row.status == ShadowStatus::Covered
                && !gaps.iter().any(|entry| {
                    entry.item_path == row.item_path
                        && entry.gap_kind == crate::shadow::ShadowGapKind::ShadowVerificationGap
                }) {
                Disposition::Exemplar
            } else if gap_paths.contains(&row.item_path)
                || gaps.iter().any(|entry| entry.item_path == row.item_path)
            {
                Disposition::Open
            } else if row.status == ShadowStatus::Extra {
                Disposition::Suppressed
            } else {
                Disposition::Exemplar
            };

            Box::new(CrossCrateShadowFinding {
                rule: ShadowPairRule,
                disposition,
                anchor,
                target_crate: upstream.to_string(),
                shadow_crate: shadow.to_string(),
                row: row.clone(),
                coverage_pct: report.coverage_pct,
            }) as Box<dyn Finding>
        })
        .collect();

    if !report.method_coverage.is_empty()
        || !report.missing_type_methods.is_empty()
        || !report.trait_coverage.is_empty()
    {
        findings.push(Box::new(ShadowMethodChecklistFinding {
            rule: ShadowPairChecklistRule,
            disposition: Disposition::Exemplar,
            anchor,
            target_crate: upstream.to_string(),
            shadow_crate: shadow.to_string(),
            body: render_shadow_method_checklist(report)?,
        }));
    }

    Ok(findings)
}
