use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use super::types::PrintRuleId;

use tracing::instrument;

#[derive(Debug, Default, Clone)]
struct PrintRow {
    crate_name: String,
    rule_id: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl PrintRow {
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
fn print_rows(findings: &[&dyn Finding]) -> Vec<PrintRow> {
    findings
        .iter()
        .filter(|finding| PrintRuleId::is_print_rule(finding.rule().id()))
        .map(|finding| PrintRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[PrintRow]) -> impl Iterator<Item = &PrintRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Writes `tracing-print.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PrintCsvReporter;

impl PrintCsvReporter {
    pub const ID: &'static str = "tracing-print-csv";
}

impl Reporter for PrintCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,rule_id,context,file,line,snippet\n");
        for row in print_rows(findings) {
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
            name: "tracing-print.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `tracing-print.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PrintChecklistReporter;

impl PrintChecklistReporter {
    pub const ID: &'static str = "tracing-print-checklist";
}

impl Reporter for PrintChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = print_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# Tracing std-print checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Replace leftover stdio with a tracing event. Per-macro knobs and \
             folder skips live under `[tracing.stdio]` in cordial.toml \
             (`println`, `eprintln`, `print`, `eprint`, `dbg`, \
             `skip_cargo_protocol`, `skip_folders`). `--apply` does not \
             rewrite these rows.\n\n",
        );
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        let mut by_rule: BTreeMap<String, Vec<&PrintRow>> = BTreeMap::new();
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
            name: "tracing-print.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `tracing-print-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PrintSummaryReporter;

impl PrintSummaryReporter {
    pub const ID: &'static str = "tracing-print-summary";
}

impl Reporter for PrintSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = print_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let total = open.len();

        let mut body = String::new();
        body.push_str("# Tracing std-print summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** leftover stdio (`println!`/`print!`/`dbg!`) sites.\n\n"
        ));
        body.push_str("| Crate | Std print |\n");
        body.push_str("| --- | ---: |\n");
        body.push_str(&format!("| `{}` | {total} |\n", ir.crate_name()));
        body.push_str(&format!("\n| **Total** | **{total}** |\n"));

        Ok(vec![Box::new(TextArtifact {
            name: "tracing-print-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
