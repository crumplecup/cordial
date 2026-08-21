use std::collections::HashMap;
use std::path::PathBuf;

use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::hooks::{AssessView, Assessor};
use crate::objects::{Disposition, FileSpan, Finding};

use super::hierarchy::{
    ModuleSizeInput, build_module_hierarchy, child_mass_list, format_mass_list, lopsided_siblings,
    top_heavy_parents, unary_nests,
};
use super::types::{ModularityFinding, ModularityKind, ModularityRule, ModuleSizeStats};

use tracing::instrument;
/// Converts modularity-site markers into open findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModularityAssessor;

impl ModularityAssessor {
    pub const ID: &'static str = "modularity-assessor";
}

struct PendingSite {
    node_id: crate::ir::NodeId,
    kind: ModularityKind,
    context: String,
    file: PathBuf,
    line: u32,
    lines: u32,
    inline: bool,
}

impl Assessor for ModularityAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &["modularity-site"]
    }

    #[instrument(level = "trace", skip(self, view))]
    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>> {
        let markers = view.markers;
        let ir = view.ir;
        let session = view.session;

        let thresholds = *crate::config::load_session_config(session).modularity();
        let mut pending = Vec::new();
        for marker in markers {
            let node_id = marker.anchor().node_id();
            let Some(node) = ir.node(node_id) else {
                continue;
            };
            let Some(kind_value) = node.attr("modularity_kind").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(kind) = ModularityKind::from_attr(kind_value) else {
                continue;
            };
            let context = node
                .attr("context")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let line = node.attr("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let lines = node.attr("lines").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let inline = node
                .attr("inline")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let file = node
                .attr("file")
                .and_then(|v| v.as_str())
                .map(|path| resolve_source_path(session, path))
                .unwrap_or_else(|| session.project_root().to_path_buf());
            pending.push(PendingSite {
                node_id,
                kind,
                context,
                file,
                line,
                lines,
                inline,
            });
        }

        let module_lines: Vec<u32> = pending
            .iter()
            .filter(|site| {
                site.kind == ModularityKind::ModuleSize
                    && site.lines >= thresholds.min_module_lines()
            })
            .map(|site| site.lines)
            .collect();
        let stats = ModuleSizeStats::from_lines(&module_lines);
        let crate_name = ir.crate_name().to_string();

        let mut findings = Vec::new();
        for site in &pending {
            let (checklist, zscore) = if site.kind == ModularityKind::ModuleSize {
                let in_sample = site.lines >= thresholds.min_module_lines();
                if in_sample {
                    let zscore = stats.zscore(site.lines);
                    (
                        thresholds.is_module_size_checklist(site.lines, zscore),
                        zscore,
                    )
                } else {
                    (false, None)
                }
            } else {
                (thresholds.is_checklist_item(site.kind, site.lines), None)
            };
            findings.push(finding_from_site(
                site,
                FindingArgs {
                    kind: site.kind,
                    crate_name: &crate_name,
                    context: site.context.clone(),
                    lines: site.lines,
                    checklist,
                    zscore,
                    share: None,
                    detail: String::new(),
                },
            ));
        }
        findings.extend(hierarchy_findings(&pending, &crate_name, thresholds));
        Ok(findings)
    }
}

#[instrument(level = "debug", skip(pending, thresholds))]
fn hierarchy_findings(
    pending: &[PendingSite],
    crate_name: &str,
    thresholds: crate::config::ModularityThresholds,
) -> Vec<Box<dyn Finding>> {
    let inputs: Vec<ModuleSizeInput> = pending
        .iter()
        .filter(|site| site.kind == ModularityKind::ModuleSize && !site.inline)
        .map(|site| ModuleSizeInput {
            path: site.context.clone(),
            file: site.file.display().to_string(),
            lines: site.lines,
        })
        .collect();
    if inputs.is_empty() {
        return Vec::new();
    }
    let tree = build_module_hierarchy(&inputs);
    let by_path: HashMap<&str, &PendingSite> = pending
        .iter()
        .filter(|site| site.kind == ModularityKind::ModuleSize && !site.inline)
        .map(|site| (site.context.as_str(), site))
        .collect();

    let mut findings = Vec::new();
    for node in top_heavy_parents(&tree) {
        if !thresholds.is_top_heavy_hit(node.own_lines, node.subtree_lines) {
            continue;
        }
        let Some(site) = by_path.get(node.path.as_str()).copied() else {
            continue;
        };
        let children = child_mass_list(node, &tree);
        let detail = if children.is_empty() {
            format!("subtree {}", node.subtree_lines)
        } else {
            format!("subtree {}; {}", node.subtree_lines, children)
        };
        findings.push(finding_from_site(
            site,
            FindingArgs {
                kind: ModularityKind::TopHeavy,
                crate_name,
                context: node.path.clone(),
                lines: node.own_lines,
                checklist: true,
                zscore: None,
                share: Some(node.top_heavy()),
                detail,
            },
        ));
    }
    for imbalance in lopsided_siblings(&tree, thresholds.hierarchy_min_lines()) {
        if !thresholds.is_lopsided_hit(imbalance.largest_subtree, imbalance.sibling_total) {
            continue;
        }
        let Some(site) = by_path.get(imbalance.largest.as_str()).copied() else {
            continue;
        };
        let detail = format!(
            "under {}; {}",
            imbalance.parent,
            format_mass_list(&imbalance.siblings)
        );
        findings.push(finding_from_site(
            site,
            FindingArgs {
                kind: ModularityKind::Lopsided,
                crate_name,
                context: imbalance.largest.clone(),
                lines: imbalance.largest_subtree,
                checklist: true,
                zscore: None,
                share: Some(imbalance.share),
                detail,
            },
        ));
    }
    for nest in unary_nests(&tree, thresholds.hierarchy_min_lines()) {
        if !thresholds.is_collapse_hit(nest.passthrough_subtree) {
            continue;
        }
        let Some(site) = by_path.get(nest.passthrough.as_str()).copied() else {
            continue;
        };
        let detail = format!(
            "under {}; lift {}",
            nest.parent,
            format_mass_list(&nest.grandchildren)
        );
        findings.push(finding_from_site(
            site,
            FindingArgs {
                kind: ModularityKind::Collapse,
                crate_name,
                context: nest.passthrough.clone(),
                lines: nest.passthrough_subtree,
                checklist: true,
                zscore: None,
                share: None,
                detail,
            },
        ));
    }
    findings
}

/// Every fact needed to build one finding from a [`PendingSite`], bundled
/// so [`finding_from_site`] takes one argument instead of eight -- named
/// fields also replace the original 9-position call's ambiguity (`true,
/// None, None` told a reader nothing about which knob was which).
struct FindingArgs<'a> {
    kind: ModularityKind,
    crate_name: &'a str,
    context: String,
    lines: u32,
    checklist: bool,
    zscore: Option<f64>,
    share: Option<f64>,
    detail: String,
}

#[instrument(level = "debug", skip(site, args))]
fn finding_from_site(site: &PendingSite, args: FindingArgs<'_>) -> Box<dyn Finding> {
    Box::new(ModularityFinding {
        rule: ModularityRule::new(args.kind),
        disposition: Disposition::Open,
        anchor: crate::objects::NodeAnchor(site.node_id),
        crate_name: args.crate_name.to_string(),
        context: args.context,
        span: FileSpan::new(site.file.clone(), site.line, 1),
        lines: args.lines,
        checklist: args.checklist,
        zscore: args.zscore,
        inline: site.inline,
        share: args.share,
        detail: args.detail,
    })
}
