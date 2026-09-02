use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use super::types::{ErrorChainProbeCounts, ErrorChainProbeId};

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct ErrorChainRow {
    crate_name: String,
    rule_id: String,
    foreign_error_type: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl ErrorChainRow {
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
            foreign_error_type: field("foreign_error_type"),
            context: field("context"),
            file: field("file"),
            line: field("line"),
            snippet: field("snippet"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn error_chain_rows(findings: &[&dyn Finding]) -> Vec<ErrorChainRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "error_chain")
        .map(|finding| ErrorChainRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[ErrorChainRow]) -> impl Iterator<Item = &ErrorChainRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Distinct crate names present in `rows`, sorted -- `view.ir.crate_name()`
/// is pinned to whichever crate the run's target discovery lists first, not
/// the crate a given row actually belongs to, so a workspace-spanning
/// artifact must derive its own crate breakdown from `row.crate_name`
/// instead (the same pattern `modularity::reporter::rows::crate_names` uses).
#[instrument(level = "debug", skip(rows))]
fn crate_names(rows: &[&ErrorChainRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

#[instrument(level = "debug", skip(rows))]
fn probe_counts_from_rows(rows: &[ErrorChainRow]) -> ErrorChainProbeCounts {
    let mut counts = ErrorChainProbeCounts::default();
    for row in rows {
        match row.rule_id.as_str() {
            s if s == ErrorChainProbeId::WrapperSourceField001.as_str() => {
                counts.wrapper_source += 1;
            }
            s if s == ErrorChainProbeId::KindWrapperPayload001.as_str() => {
                counts.kind_wrapper_payload += 1;
            }
            s if s == ErrorChainProbeId::FromBridge001.as_str() => counts.from_bridge += 1,
            s if s == ErrorChainProbeId::PreservedQuestionMark001.as_str() => {
                counts.preserved_question_mark += 1;
            }
            s if s == ErrorChainProbeId::PreservedMapErr001.as_str() => {
                counts.preserved_map_err += 1;
            }
            _ => {}
        }
    }
    counts
}

/// Writes `error-chain-preserved.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorChainCsvReporter;

impl ErrorChainCsvReporter {
    pub const ID: &'static str = "error-chain-csv";
}

impl Reporter for ErrorChainCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,rule_id,foreign_error_type,context,file,line,snippet\n");
        for row in error_chain_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.rule_id),
                csv_field(&row.foreign_error_type),
                csv_field(&row.context),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "error-chain-preserved.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `error-chain-preserved.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorChainChecklistReporter;

impl ErrorChainChecklistReporter {
    pub const ID: &'static str = "error-chain-checklist";
}

impl Reporter for ErrorChainChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = error_chain_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let counts = probe_counts_from_rows(&rows);
        let propagation = counts.preserved_propagation();

        let mut body = String::new();
        body.push_str("# Error chain preserved checklist\n\n");
        body.push_str(&format!(
            "**Items:** {} (**{propagation}** propagation sites via `?`)\n\n",
            open.len()
        ));
        body.push_str(
            "Foreign errors wrapped in a `source`-bearing type, carried by an umbrella \
             `ErrorKind` payload, and propagated with `?` (directly or after chain-preserving \
             `map_err`). These are **reference patterns** for error-chain preservation. \
             Contrast with `foreign-error-types.checklist.md` (chain breaks).\n\n",
        );

        for crate_name in crate_names(&open) {
            let crate_open: Vec<_> = open
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            body.push_str(&format!("## `{crate_name}`\n\n"));

            let mut by_rule: BTreeMap<String, Vec<&ErrorChainRow>> = BTreeMap::new();
            for row in &crate_open {
                by_rule.entry(row.rule_id.clone()).or_default().push(row);
            }

            for (rule_id, entries) in by_rule {
                body.push_str(&format!("### {rule_id}\n\n"));
                for entry in entries {
                    let foreign = if entry.foreign_error_type.is_empty() {
                        "—"
                    } else {
                        &entry.foreign_error_type
                    };
                    body.push_str(&format!(
                        "- [x] `{}` — `{}:{}` — foreign `{foreign}` — `{}`\n",
                        entry.context, entry.file, entry.line, entry.snippet
                    ));
                }
                body.push('\n');
            }
        }

        Ok(vec![Box::new(TextArtifact {
            name: "error-chain-preserved.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `error-chain-preserved-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorChainSummaryReporter;

impl ErrorChainSummaryReporter {
    pub const ID: &'static str = "error-chain-summary";
}

impl Reporter for ErrorChainSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = error_chain_rows(findings);
        let counts = probe_counts_from_rows(&rows);
        let total = counts.total();
        let propagation = counts.preserved_propagation();
        let infrastructure = counts.infrastructure();

        let mut body = String::new();
        body.push_str("# Error chain preserved summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** preserved patterns — **{propagation}** propagation \
             sites, **{infrastructure}** infrastructure (wrapper / kind / `From` bridge).\n\n"
        ));
        body.push_str("| Crate | Total | Propagation | Infrastructure |\n");
        body.push_str("| --- | ---: | ---: | ---: |\n");
        let all_rows: Vec<&ErrorChainRow> = rows.iter().collect();
        for crate_name in crate_names(&all_rows) {
            let crate_rows: Vec<ErrorChainRow> = rows
                .iter()
                .filter(|row| row.crate_name == crate_name)
                .cloned()
                .collect();
            let crate_counts = probe_counts_from_rows(&crate_rows);
            body.push_str(&format!(
                "| `{crate_name}` | {} | {} | {} |\n",
                crate_counts.total(),
                crate_counts.preserved_propagation(),
                crate_counts.infrastructure(),
            ));
        }
        body.push_str(&format!(
            "\n| **Total** | **{total}** | **{propagation}** | **{infrastructure}** |\n"
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "error-chain-preserved-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
