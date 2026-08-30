use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;

#[derive(Debug, Default, Clone)]
struct DocWarningRow {
    crate_name: String,
    rule_id: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl DocWarningRow {
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
            context: field("context"),
            file: field("file"),
            line: field("line"),
            snippet: field("snippet"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn warning_rows(findings: &[&dyn Finding]) -> Vec<DocWarningRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "doc_warnings")
        .map(|finding| DocWarningRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[DocWarningRow]) -> impl Iterator<Item = &DocWarningRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Writes `doc-warnings.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DocWarningCsvReporter;

impl DocWarningCsvReporter {
    /// Stable identifier for `DocWarningCsvReporter`.
    pub const ID: &'static str = "doc-warning-csv";
}

impl Reporter for DocWarningCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,rule_id,lint,file,line,message\n");
        for row in warning_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.rule_id),
                csv_field(&row.context),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "doc-warnings.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `doc-warnings.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DocWarningChecklistReporter;

impl DocWarningChecklistReporter {
    /// Stable identifier for `DocWarningChecklistReporter`.
    pub const ID: &'static str = "doc-warning-checklist";
}

impl Reporter for DocWarningChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = warning_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# rustdoc warnings checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "`cargo check` / clippy never run rustdoc. Each `rustdoc::*` \
             diagnostic from `cargo doc` is an action item (broken intra-doc \
             links, invalid HTML, …). CI that sets `RUSTDOCFLAGS=-D warnings` \
             fails the build on these.\n\n",
        );
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        let mut by_lint: BTreeMap<String, Vec<&DocWarningRow>> = BTreeMap::new();
        for row in &open {
            by_lint.entry(row.context.clone()).or_default().push(row);
        }

        for (lint, entries) in by_lint {
            body.push_str(&format!("### `{lint}`\n\n"));
            for entry in entries {
                body.push_str(&format!(
                    "- [ ] `{}:{}` — {}\n",
                    entry.file, entry.line, entry.snippet
                ));
            }
            body.push('\n');
        }

        Ok(vec![Box::new(TextArtifact {
            name: "doc-warnings.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `doc-warnings-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DocWarningSummaryReporter;

impl DocWarningSummaryReporter {
    /// Stable identifier for `DocWarningSummaryReporter`.
    pub const ID: &'static str = "doc-warning-summary";
}

impl Reporter for DocWarningSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = warning_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let total = open.len();

        let mut body = String::new();
        body.push_str("# rustdoc warnings summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** rustdoc (`cargo doc`) warnings.\n\n"
        ));
        body.push_str("| Crate | rustdoc warnings |\n");
        body.push_str("| --- | ---: |\n");
        body.push_str(&format!("| `{}` | {total} |\n", ir.crate_name()));
        body.push_str(&format!("\n| **Total** | **{total}** |\n"));

        Ok(vec![Box::new(TextArtifact {
            name: "doc-warnings-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
