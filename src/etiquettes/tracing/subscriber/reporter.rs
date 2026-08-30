use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use super::types::SubscriberRuleId;

use tracing::instrument;

#[derive(Debug, Default, Clone)]
struct SubscriberRow {
    crate_name: String,
    rule_id: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl SubscriberRow {
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
fn subscriber_rows(findings: &[&dyn Finding]) -> Vec<SubscriberRow> {
    findings
        .iter()
        .filter(|finding| SubscriberRuleId::is_subscriber_rule(finding.rule().id()))
        .map(|finding| SubscriberRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[SubscriberRow]) -> impl Iterator<Item = &SubscriberRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Writes `tracing-subscriber.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SubscriberCsvReporter;

impl SubscriberCsvReporter {
    pub const ID: &'static str = "tracing-subscriber-csv";
}

impl Reporter for SubscriberCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,rule_id,context,file,line,snippet\n");
        for row in subscriber_rows(findings) {
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
            name: "tracing-subscriber.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `tracing-subscriber.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SubscriberChecklistReporter;

impl SubscriberChecklistReporter {
    pub const ID: &'static str = "tracing-subscriber-checklist";
}

impl Reporter for SubscriberChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = subscriber_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# Tracing subscriber checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Install one library helper that reads `RUST_LOG` with a fallback and \
             uses `try_init()` (or `Once`). Call it from `fn main` and from each \
             `#[test]` under `tests/`. `--apply` does not rewrite these rows.\n\n",
        );
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        let mut by_rule: BTreeMap<String, Vec<&SubscriberRow>> = BTreeMap::new();
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
            name: "tracing-subscriber.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `tracing-subscriber-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SubscriberSummaryReporter;

impl SubscriberSummaryReporter {
    pub const ID: &'static str = "tracing-subscriber-summary";
}

impl Reporter for SubscriberSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = subscriber_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let total = open.len();

        let mut body = String::new();
        body.push_str("# Tracing subscriber summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** subscriber-init gaps.\n\n"
        ));
        body.push_str("| Crate | Subscriber gaps |\n");
        body.push_str("| --- | ---: |\n");
        body.push_str(&format!("| `{}` | {total} |\n", ir.crate_name()));
        body.push_str(&format!("\n| **Total** | **{total}** |\n"));

        Ok(vec![Box::new(TextArtifact {
            name: "tracing-subscriber-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
