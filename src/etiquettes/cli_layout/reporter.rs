use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct CliLayoutRow {
    crate_name: String,
    rule_id: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl CliLayoutRow {
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
fn cli_layout_rows(findings: &[&dyn Finding]) -> Vec<CliLayoutRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "cli_layout")
        .map(|finding| CliLayoutRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[CliLayoutRow]) -> impl Iterator<Item = &CliLayoutRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Distinct crate names present in `rows`, sorted -- `view.ir.crate_name()`
/// is pinned to whichever crate the run's target discovery lists first, not
/// the crate a given row actually belongs to, so a workspace-spanning
/// artifact must derive its own crate breakdown from `row.crate_name`
/// instead (the same pattern `modularity::reporter::rows::crate_names` uses).
#[instrument(level = "debug", skip(rows))]
fn crate_names(rows: &[&CliLayoutRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

/// Writes `cli-layout.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CliLayoutCsvReporter;

impl CliLayoutCsvReporter {
    pub const ID: &'static str = "cli-layout-csv";
}

impl Reporter for CliLayoutCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let all_rows = cli_layout_rows(findings);
        let rows: Vec<_> = open_rows(&all_rows).collect();

        let mut body = String::from("crate,rule_id,context,file,line,snippet\n");
        for row in rows {
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
            name: "cli-layout.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `cli-layout.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CliLayoutChecklistReporter;

impl CliLayoutChecklistReporter {
    pub const ID: &'static str = "cli-layout-checklist";
}

impl Reporter for CliLayoutChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = cli_layout_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();

        let mut body = String::new();
        body.push_str("# CLI layout checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Clap types and dispatch belong in the library. Each `Parser` / `Subcommand` \
             implements `act` and hands off to nested clap types. `main` only parses, \
             calls `act`, and converts with miette.\n\n",
        );

        if !open.is_empty() {
            for crate_name in crate_names(&open) {
                let crate_open: Vec<_> = open
                    .iter()
                    .copied()
                    .filter(|row| row.crate_name == crate_name)
                    .collect();
                body.push_str(&format!("## `{crate_name}`\n\n"));
                for row in &crate_open {
                    body.push_str(&format!(
                        "- [ ] `{}:{}` — `{}` — {}\n  - {}\n",
                        row.file, row.line, row.rule_id, row.context, row.snippet
                    ));
                }
                body.push('\n');
            }
        } else {
            body.push_str("_No CLI layout violations found._\n\n");
        }

        Ok(vec![Box::new(TextArtifact {
            name: "cli-layout.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `cli-layout-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CliLayoutSummaryReporter;

impl CliLayoutSummaryReporter {
    pub const ID: &'static str = "cli-layout-summary";
}

impl Reporter for CliLayoutSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = cli_layout_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let islands = open
            .iter()
            .filter(|row| row.rule_id == "CLI-ISLAND-001")
            .count();
        let acts = open
            .iter()
            .filter(|row| row.rule_id == "CLI-ACT-001")
            .count();
        let mains = open
            .iter()
            .filter(|row| row.rule_id == "CLI-MAIN-001")
            .count();

        let mut body = String::new();
        body.push_str("# CLI layout summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Open items: **{}** (`CLI-ISLAND-001`: {islands}, `CLI-ACT-001`: {acts}, `CLI-MAIN-001`: {mains}).\n",
            open.len()
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "cli-layout-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
