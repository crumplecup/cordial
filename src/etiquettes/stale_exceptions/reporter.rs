use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Disposition, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;

#[derive(Debug, Default, Clone)]
struct StaleRow {
    crate_name: String,
    etiquette: String,
    expected_rule: String,
    file: String,
    line: String,
}

impl StaleRow {
    #[instrument(level = "debug", skip(finding))]
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
            etiquette: field("etiquette"),
            expected_rule: field("expected_rule"),
            file: field("file"),
            line: field("line"),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn open_rows(findings: &[&dyn Finding]) -> Vec<StaleRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "stale_exceptions")
        .filter(|finding| finding.disposition() == Disposition::Open)
        .map(|finding| StaleRow::from_finding(*finding))
        .collect()
}

/// Writes `stale-exceptions.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct StaleExceptionChecklistReporter;

impl Reporter for StaleExceptionChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "stale-exception-checklist"
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = open_rows(view.findings());
        let mut body = String::from("# Stale exceptions checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", rows.len()));
        body.push_str(
            "Each row below is an exception that suppressed no finding in this run. \
             Re-key it with `cordial exceptions edit` if the code moved, or drop it \
             with `cordial exceptions remove` if the finding is gone.\n\n",
        );
        let mut by_crate: BTreeMap<&str, Vec<&StaleRow>> = BTreeMap::new();
        for row in &rows {
            by_crate.entry(&row.crate_name).or_default().push(row);
        }
        for (crate_name, crate_rows) in by_crate {
            body.push_str(&format!("## `{crate_name}`\n\n### EXCEPTION-STALE-001\n\n"));
            for row in crate_rows {
                body.push_str(&format!(
                    "- [ ] `{}` exception for `{}` — `{}:{}`\n",
                    row.etiquette, row.expected_rule, row.file, row.line
                ));
            }
            body.push('\n');
        }
        Ok(vec![Box::new(TextArtifact::new(
            "stale-exceptions.checklist.md".to_string(),
            "text/markdown".to_string(),
            body,
        ))])
    }
}

/// Writes `stale-exceptions-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct StaleExceptionSummaryReporter;

impl Reporter for StaleExceptionSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "stale-exception-summary"
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = open_rows(view.findings());
        let mut body = String::from("# Stale exceptions summary\n\n---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{}** stale exception rows.\n\n",
            rows.len()
        ));
        body.push_str("| Etiquette | Stale rows |\n| --- | ---: |\n");
        let mut by_etiquette: BTreeMap<&str, usize> = BTreeMap::new();
        for row in &rows {
            *by_etiquette.entry(&row.etiquette).or_default() += 1;
        }
        for (etiquette, count) in by_etiquette {
            body.push_str(&format!("| `{etiquette}` | {count} |\n"));
        }
        Ok(vec![Box::new(TextArtifact::new(
            "stale-exceptions-summary.md".to_string(),
            "text/markdown".to_string(),
            body,
        ))])
    }
}
