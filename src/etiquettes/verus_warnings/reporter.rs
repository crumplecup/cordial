use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;

#[derive(Debug, Default, Clone)]
struct VerusWarningRow {
    crate_name: String,
    rule_id: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl VerusWarningRow {
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
fn warning_rows(findings: &[&dyn Finding]) -> Vec<VerusWarningRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "verus_warnings")
        .map(|finding| VerusWarningRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[VerusWarningRow]) -> impl Iterator<Item = &VerusWarningRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Writes `verus-warnings.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct VerusWarningCsvReporter;

impl VerusWarningCsvReporter {
    pub const ID: &'static str = "verus-warning-csv";
}

impl Reporter for VerusWarningCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,rule_id,context,file,line,snippet\n");
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
            name: "verus-warnings.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `verus-warnings.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct VerusWarningChecklistReporter;

impl VerusWarningChecklistReporter {
    pub const ID: &'static str = "verus-warning-checklist";
}

impl Reporter for VerusWarningChecklistReporter {
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
        body.push_str("# Verus compiler warnings checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Verus is a rustc fork and has no deny-warnings flag. Each \
             `warning:` in its compiler output is an action item — rustc and \
             clippy never see these diagnostics.\n\n",
        );
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        let mut by_rule: BTreeMap<String, Vec<&VerusWarningRow>> = BTreeMap::new();
        for row in &open {
            by_rule.entry(row.rule_id.clone()).or_default().push(row);
        }

        for (rule_id, entries) in by_rule {
            body.push_str(&format!("### {rule_id}\n\n"));
            for entry in entries {
                body.push_str(&format!(
                    "- [ ] `{}` — `{}:{}` — `{}`\n",
                    entry.context, entry.file, entry.line, entry.snippet
                ));
            }
            body.push('\n');
        }

        Ok(vec![Box::new(TextArtifact {
            name: "verus-warnings.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `verus-warnings-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct VerusWarningSummaryReporter;

impl VerusWarningSummaryReporter {
    pub const ID: &'static str = "verus-warning-summary";
}

impl Reporter for VerusWarningSummaryReporter {
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
        body.push_str("# Verus compiler warnings summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** Verus compiler warnings.\n\n"
        ));
        body.push_str("| Crate | Verus warnings |\n");
        body.push_str("| --- | ---: |\n");
        body.push_str(&format!("| `{}` | {total} |\n", ir.crate_name()));
        body.push_str(&format!("\n| **Total** | **{total}** |\n"));

        Ok(vec![Box::new(TextArtifact {
            name: "verus-warnings-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
