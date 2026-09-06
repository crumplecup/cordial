use crate::enricher::resolve_source_path;
use crate::error::CordialResult;
use crate::hooks::{AssessView, Assessor};
use crate::objects::{Disposition, FileSpan, Finding};

use super::types::{
    DependencyFreshnessFinding, DependencyFreshnessRule, DependencyFreshnessRuleId,
};

use tracing::instrument;

/// Converts dependency freshness markers into open findings.
#[derive(Debug, Default, Clone, Copy)]
pub struct DependencyFreshnessAssessor;

impl DependencyFreshnessAssessor {
    pub const ID: &'static str = "dependency-freshness-assessor";
}

impl Assessor for DependencyFreshnessAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &["dependency-freshness-site"]
    }

    #[instrument(level = "trace", skip(self, view))]
    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>> {
        let mut findings = Vec::new();
        let policy = crate::config::load_session_config(view.session)
            .dependency_freshness()
            .clone();
        for marker in view.markers {
            let node_id = marker.anchor().node_id();
            let Some(node) = view.ir.node(node_id) else {
                continue;
            };
            let Some(rule_ids) = node
                .attr("dependency_freshness_rule_ids")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let dependency_name = node_attr(&node, "dependency_name");
            let package_name = node_attr(&node, "package_name");
            let locked_versions = node_attr(&node, "locked_versions");
            let available_versions = node_attr(&node, "available_versions");
            let update_kinds = node_attr(&node, "update_kinds");
            let section = node_attr(&node, "section");
            let version_spec = node_attr(&node, "version_spec");
            let line = node
                .attr("line")
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as u32;
            let file = node
                .attr("manifest_path")
                .and_then(serde_json::Value::as_str)
                .map(|path| resolve_source_path(view.session, path))
                .unwrap_or_else(|| view.session.project_root().join("Cargo.toml"));
            let snippet = format!(
                "{dependency_name} locked {locked_versions}; available {available_versions}"
            );

            for rule_id in rule_ids
                .split('|')
                .filter_map(DependencyFreshnessRuleId::from_attr)
            {
                if !policy.rule_enabled(rule_id.as_str()) {
                    continue;
                }
                findings.push(Box::new(
                    DependencyFreshnessFinding::builder()
                        .rule(DependencyFreshnessRule::new(rule_id))
                        .disposition(Disposition::Open)
                        .anchor(crate::objects::NodeAnchor(node_id))
                        .crate_name(view.ir.crate_name().to_string())
                        .dependency_name(dependency_name.clone())
                        .package_name(package_name.clone())
                        .span(FileSpan::new(file.clone(), line, 1))
                        .section(section.clone())
                        .version_spec(version_spec.clone())
                        .locked_versions(locked_versions.clone())
                        .available_versions(available_versions.clone())
                        .update_kinds(update_kinds.clone())
                        .snippet(snippet.clone())
                        .build()?,
                ) as Box<dyn Finding>);
            }
        }
        Ok(findings)
    }
}

fn node_attr(node: &crate::ir::NodeRef<'_>, key: &str) -> String {
    node.attr(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}
