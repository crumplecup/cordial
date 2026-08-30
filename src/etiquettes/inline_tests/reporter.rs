use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;

#[derive(Debug, Default, Clone)]
struct InlineTestRow {
    crate_name: String,
    rule_id: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl InlineTestRow {
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
fn inline_test_rows(findings: &[&dyn Finding]) -> Vec<InlineTestRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "inline_tests")
        .map(|finding| InlineTestRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[InlineTestRow]) -> impl Iterator<Item = &InlineTestRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Writes `inline-tests.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InlineTestCsvReporter;

impl InlineTestCsvReporter {
    pub const ID: &'static str = "inline-test-csv";
}

impl Reporter for InlineTestCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,rule_id,context,file,line,snippet\n");
        for row in inline_test_rows(findings) {
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
            name: "inline-tests.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `inline-tests.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InlineTestChecklistReporter;

impl InlineTestChecklistReporter {
    pub const ID: &'static str = "inline-test-checklist";
}

impl Reporter for InlineTestChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = inline_test_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# Inline tests checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Move tests out of `src/` into the crate `tests/` directory so library \
             modules stay production code.\n\n",
        );
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        let mut by_rule: BTreeMap<String, Vec<&InlineTestRow>> = BTreeMap::new();
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
            name: "inline-tests.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `inline-tests-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InlineTestSummaryReporter;

impl InlineTestSummaryReporter {
    pub const ID: &'static str = "inline-test-summary";
}

impl Reporter for InlineTestSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = inline_test_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let total = open.len();
        let mods = open
            .iter()
            .filter(|row| row.rule_id == "INLINE-TEST-MOD")
            .count();
        let cfg = open
            .iter()
            .filter(|row| row.rule_id == "INLINE-TEST-CFG")
            .count();
        let fns = open
            .iter()
            .filter(|row| row.rule_id == "INLINE-TEST-FN")
            .count();

        let mut body = String::new();
        body.push_str("# Inline tests summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** sites — modules **{mods}**, other `#[cfg(test)]` **{cfg}**, \
             free `#[test]` **{fns}**.\n\n"
        ));
        body.push_str("| Crate | Total | Modules | cfg(test) items | #[test] fns |\n");
        body.push_str("| --- | ---: | ---: | ---: | ---: |\n");
        body.push_str(&format!(
            "| `{}` | {total} | {mods} | {cfg} | {fns} |\n",
            ir.crate_name()
        ));
        body.push_str(&format!(
            "\n| **Total** | **{total}** | **{mods}** | **{cfg}** | **{fns}** |\n"
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "inline-tests-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
