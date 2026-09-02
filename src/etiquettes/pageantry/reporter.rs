use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;

#[derive(Debug, Default, Clone)]
struct PageantryRow {
    crate_name: String,
    rule_id: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl PageantryRow {
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
fn pageantry_rows(findings: &[&dyn Finding]) -> Vec<PageantryRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "pageantry")
        .map(|finding| PageantryRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[PageantryRow]) -> impl Iterator<Item = &PageantryRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Distinct crate names present in `rows`, sorted -- `view.ir.crate_name()`
/// is pinned to whichever crate the run's target discovery lists first, not
/// the crate a given row actually belongs to, so a workspace-spanning
/// artifact must derive its own crate breakdown from `row.crate_name`
/// instead (the same pattern `modularity::reporter::rows::crate_names` uses).
#[instrument(level = "debug", skip(rows))]
fn crate_names(rows: &[&PageantryRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

/// Writes `pageantry.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PageantryCsvReporter;

impl PageantryCsvReporter {
    pub const ID: &'static str = "pageantry-csv";
}

impl Reporter for PageantryCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,rule_id,context,file,line,snippet\n");
        for row in pageantry_rows(findings) {
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
            name: "pageantry.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `pageantry.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PageantryChecklistReporter;

impl PageantryChecklistReporter {
    pub const ID: &'static str = "pageantry-checklist";
}

impl Reporter for PageantryChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = pageantry_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# Pageantry checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Move each trait to the leading block just below the import / `mod` \
             header. A run of traits at the top is fine; a trait after types \
             have already started is not.\n\n",
        );

        for crate_name in crate_names(&open) {
            let crate_open: Vec<_> = open
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            body.push_str(&format!("## `{crate_name}`\n\n"));

            let mut by_rule: BTreeMap<String, Vec<&PageantryRow>> = BTreeMap::new();
            for row in &crate_open {
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
        }

        Ok(vec![Box::new(TextArtifact {
            name: "pageantry.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `pageantry-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PageantrySummaryReporter;

impl PageantrySummaryReporter {
    pub const ID: &'static str = "pageantry-summary";
}

impl Reporter for PageantrySummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = pageantry_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let total = open.len();

        let mut body = String::new();
        body.push_str("# Pageantry summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** traits after the leading block.\n\n"
        ));
        body.push_str("| Crate | Misplaced traits |\n");
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
            name: "pageantry-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
