use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use super::types::InternalErrorRecordKind;

mod checklist;
mod counts;
mod summary;
pub use checklist::InternalErrorChainChecklistReporter;
use counts::compliance_counts;
pub use summary::InternalErrorChainSummaryReporter;

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct InternalErrorChainRow {
    crate_name: String,
    record_kind: String,
    rule_id: String,
    context: String,
    type_path: String,
    node_class: String,
    source_target: String,
    reaches_foreign: String,
    chain_depth: String,
    foreign_error_type: String,
    internal_constructor: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl InternalErrorChainRow {
    #[instrument(level = "debug", skip(finding), ret)]
    fn from_finding(finding: &dyn Finding) -> Self {
        let mut sink = MapFindingSink::default();
        finding.emit(&mut sink);
        let field = |name: &str| {
            sink.fields()
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        Self {
            crate_name: field("crate"),
            record_kind: field("record_kind"),
            rule_id: field("rule_id"),
            context: field("context"),
            type_path: field("type_path"),
            node_class: field("node_class"),
            source_target: field("source_target"),
            reaches_foreign: field("reaches_foreign"),
            chain_depth: field("chain_depth"),
            foreign_error_type: field("foreign_error_type"),
            internal_constructor: field("internal_constructor"),
            file: field("file"),
            line: field("line"),
            snippet: field("snippet"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn internal_error_chain_rows(findings: &[&dyn Finding]) -> Vec<InternalErrorChainRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "internal_error_chain")
        .map(|finding| InternalErrorChainRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn type_graph_rows(rows: &[InternalErrorChainRow]) -> impl Iterator<Item = &InternalErrorChainRow> {
    rows.iter()
        .filter(|row| row.record_kind == InternalErrorRecordKind::TypeGraph.as_str())
}

#[instrument(level = "debug", skip(rows))]
fn compliance_rows(rows: &[InternalErrorChainRow]) -> impl Iterator<Item = &InternalErrorChainRow> {
    rows.iter().filter(|row| {
        row.record_kind == InternalErrorRecordKind::Compliance.as_str() && row.disposition == "open"
    })
}

/// Distinct crate names present in `rows`, sorted -- `view.ir().crate_name()`
/// is pinned to whichever crate the run's target discovery lists first, not
/// the crate a given row actually belongs to, so a workspace-spanning
/// artifact must derive its own crate breakdown from `row.crate_name`
/// instead (the same pattern `modularity::reporter::rows::crate_names` uses).
#[instrument(level = "debug", skip(rows))]
fn crate_names(rows: &[&InternalErrorChainRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

#[instrument(level = "debug", skip(rows))]
fn class_counts(rows: &[InternalErrorChainRow]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for row in type_graph_rows(rows) {
        if row.node_class.is_empty() {
            continue;
        }
        *counts.entry(row.node_class.clone()).or_default() += 1;
    }
    counts
}

/// Writes `internal-error-type-graph.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InternalErrorTypeGraphCsvReporter;

impl InternalErrorTypeGraphCsvReporter {
    pub const ID: &'static str = "internal-error-type-graph-csv";
}

impl Reporter for InternalErrorTypeGraphCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings();

        let rows = internal_error_chain_rows(findings);
        let mut body = String::from(
            "crate,type_path,node_class,probe_id,source_target,reaches_foreign,chain_depth,file,line,snippet\n",
        );
        for row in type_graph_rows(&rows) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.type_path),
                csv_field(&row.node_class),
                csv_field(&row.rule_id),
                csv_field(&row.source_target),
                csv_field(&row.reaches_foreign),
                csv_field(&row.chain_depth),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact::new(
            "internal-error-type-graph.csv".to_string(),
            "text/csv".to_string(),
            body,
        ))])
    }
}

/// Writes `internal-error-compliance.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InternalErrorComplianceCsvReporter;

impl InternalErrorComplianceCsvReporter {
    pub const ID: &'static str = "internal-error-compliance-csv";
}

impl Reporter for InternalErrorComplianceCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings();

        let rows = internal_error_chain_rows(findings);
        let mut body = String::from(
            "crate,rule_id,foreign_error_type,internal_constructor,context,file,line,snippet\n",
        );
        for row in compliance_rows(&rows) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.rule_id),
                csv_field(&row.foreign_error_type),
                csv_field(&row.internal_constructor),
                csv_field(&row.context),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact::new(
            "internal-error-compliance.csv".to_string(),
            "text/csv".to_string(),
            body,
        ))])
    }
}
