use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::ir::{NodeKind, QueryBuilder};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use super::enricher::DependencyFreshnessSurveyEnricher;

use tracing::instrument;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DependencyFreshnessSurveyRow {
    crate_name: String,
    dependency_name: String,
    package_name: String,
    manifest_path: String,
    line: String,
    section: String,
    version_spec: String,
    source_kind: String,
    locked_versions: String,
    available_versions: String,
    update_kinds: String,
    rule_ids: String,
    indicators: String,
}

impl DependencyFreshnessSurveyRow {
    fn from_node(node: crate::ir::NodeRef<'_>) -> Self {
        let field = |key: &str| {
            node.attr(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Self {
            crate_name: field("crate"),
            dependency_name: field("dependency_name"),
            package_name: field("package_name"),
            manifest_path: field("manifest_path"),
            line: node
                .attr("line")
                .and_then(serde_json::Value::as_u64)
                .map(|line| line.to_string())
                .unwrap_or_default(),
            section: field("section"),
            version_spec: field("version_spec"),
            source_kind: field("source_kind"),
            locked_versions: field("locked_versions"),
            available_versions: field("available_versions"),
            update_kinds: field("update_kinds"),
            rule_ids: field("dependency_freshness_rule_ids"),
            indicators: field("indicators"),
        }
    }
}

#[instrument(level = "debug", skip(view))]
fn survey_rows(view: RenderView<'_>) -> Vec<DependencyFreshnessSurveyRow> {
    let query = QueryBuilder::new()
        .node_kinds([NodeKind::Plugin("dependency_freshness".to_string())])
        .has_attr(DependencyFreshnessSurveyEnricher::ATTR_NODE_KIND)
        .build();
    let mut rows: Vec<_> = view
        .ir
        .nodes_matching(&query)
        .into_iter()
        .map(DependencyFreshnessSurveyRow::from_node)
        .collect();
    rows.sort();
    rows
}

/// Writes `dependency-freshness-survey.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DependencyFreshnessSurveyReporter;

impl DependencyFreshnessSurveyReporter {
    /// Stable reporter id.
    pub const ID: &'static str = "dependency-freshness-survey-csv";
}

impl Reporter for DependencyFreshnessSurveyReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let mut body = String::from(
            "crate,dependency,package,manifest,line,section,version_spec,source_kind,locked_versions,available_versions,update_kinds,rule_ids,indicators\n",
        );
        for row in survey_rows(view) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.dependency_name),
                csv_field(&row.package_name),
                csv_field(&row.manifest_path),
                csv_field(&row.line),
                csv_field(&row.section),
                csv_field(&row.version_spec),
                csv_field(&row.source_kind),
                csv_field(&row.locked_versions),
                csv_field(&row.available_versions),
                csv_field(&row.update_kinds),
                csv_field(&row.rule_ids),
                csv_field(&row.indicators),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "dependency-freshness-survey.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct DependencyFreshnessRow {
    crate_name: String,
    rule_id: String,
    dependency: String,
    package: String,
    file: String,
    line: String,
    section: String,
    version_spec: String,
    locked_versions: String,
    available_versions: String,
    update_kinds: String,
    snippet: String,
    disposition: String,
}

impl DependencyFreshnessRow {
    #[instrument(level = "debug", skip(finding), ret)]
    fn from_finding(finding: &dyn Finding) -> Self {
        let mut sink = MapFindingSink::default();
        finding.emit(&mut sink);
        let field = |name: &str| {
            sink.fields
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        Self {
            crate_name: field("crate"),
            rule_id: field("rule_id"),
            dependency: field("dependency"),
            package: field("package"),
            file: field("file"),
            line: field("line"),
            section: field("section"),
            version_spec: field("version_spec"),
            locked_versions: field("locked_versions"),
            available_versions: field("available_versions"),
            update_kinds: field("update_kinds"),
            snippet: field("snippet"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn finding_rows(findings: &[&dyn Finding]) -> Vec<DependencyFreshnessRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "dependency_freshness")
        .map(|finding| DependencyFreshnessRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[DependencyFreshnessRow]) -> impl Iterator<Item = &DependencyFreshnessRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

#[instrument(level = "debug", skip(rows))]
fn crate_names(rows: &[&DependencyFreshnessRow]) -> Vec<String> {
    let mut names: Vec<_> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

/// Writes `dependency-freshness.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DependencyFreshnessCsvReporter;

impl DependencyFreshnessCsvReporter {
    /// Stable reporter id.
    pub const ID: &'static str = "dependency-freshness-csv";
}

impl Reporter for DependencyFreshnessCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let mut body = String::from(
            "crate,rule_id,dependency,package,file,line,section,version_spec,locked_versions,available_versions,update_kinds,snippet\n",
        );
        for row in finding_rows(view.findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.rule_id),
                csv_field(&row.dependency),
                csv_field(&row.package),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.section),
                csv_field(&row.version_spec),
                csv_field(&row.locked_versions),
                csv_field(&row.available_versions),
                csv_field(&row.update_kinds),
                csv_field(&row.snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "dependency-freshness.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `dependency-freshness.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DependencyFreshnessChecklistReporter;

impl DependencyFreshnessChecklistReporter {
    /// Stable reporter id.
    pub const ID: &'static str = "dependency-freshness-checklist";
}

impl Reporter for DependencyFreshnessChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = finding_rows(view.findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# Dependency freshness checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Review available dependency updates. Patch, minor, and major drift \
             remain separate rule ids so CI can deny them independently.\n\n",
        );

        for crate_name in crate_names(&open) {
            let crate_open: Vec<_> = open
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            body.push_str(&format!("## `{crate_name}`\n\n"));

            for row in crate_open {
                body.push_str(&format!(
                    "- [ ] `{}` — `{}` → `{}` — `{}`\n",
                    row.rule_id, row.locked_versions, row.available_versions, row.package
                ));
            }
            body.push('\n');
        }

        Ok(vec![Box::new(TextArtifact {
            name: "dependency-freshness.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `dependency-freshness-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DependencyFreshnessSummaryReporter;

impl DependencyFreshnessSummaryReporter {
    /// Stable reporter id.
    pub const ID: &'static str = "dependency-freshness-summary";
}

impl Reporter for DependencyFreshnessSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = finding_rows(view.findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let total = open.len();
        let mut body = String::new();
        body.push_str("# Dependency freshness summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** dependency freshness items.\n\n"
        ));
        body.push_str("| Crate | Dependency freshness |\n");
        body.push_str("| --- | ---: |\n");
        for crate_name in crate_names(&open) {
            let crate_total = open
                .iter()
                .filter(|row| row.crate_name == crate_name)
                .count();
            body.push_str(&format!("| `{crate_name}` | {crate_total} |\n"));
        }
        body.push_str(&format!("\n| **Total** | **{total}** |\n"));

        Ok(vec![Box::new(TextArtifact {
            name: "dependency-freshness-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
