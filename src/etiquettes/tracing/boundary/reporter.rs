use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use super::types::BoundaryRuleId;

use tracing::instrument;

#[derive(Debug, Default, Clone)]
struct BoundaryRow {
    crate_name: String,
    rule_id: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl BoundaryRow {
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
fn boundary_rows(findings: &[&dyn Finding]) -> Vec<BoundaryRow> {
    findings
        .iter()
        .filter(|finding| BoundaryRuleId::is_boundary_rule(finding.rule().id()))
        .map(|finding| BoundaryRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[BoundaryRow]) -> impl Iterator<Item = &BoundaryRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Distinct crate names present in `rows`, sorted -- `view.ir.crate_name()`
/// is pinned to whichever crate the run's target discovery lists first, not
/// the crate a given row actually belongs to, so a workspace-spanning
/// artifact must derive its own crate breakdown from `row.crate_name`
/// instead (the same pattern `modularity::reporter::rows::crate_names` uses).
#[instrument(level = "debug", skip(rows))]
fn crate_names(rows: &[&BoundaryRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

/// Writes `tracing-boundary.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct BoundaryCsvReporter;

impl BoundaryCsvReporter {
    pub const ID: &'static str = "tracing-boundary-csv";
}

impl Reporter for BoundaryCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,rule_id,context,file,line,snippet\n");
        for row in boundary_rows(findings) {
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
            name: "tracing-boundary.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `tracing-boundary.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct BoundaryChecklistReporter;

impl BoundaryChecklistReporter {
    pub const ID: &'static str = "tracing-boundary-checklist";
}

impl Reporter for BoundaryChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = boundary_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# Binary error-boundary checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "A library may bubble errors up via `?` — that's the existing error-chain \
             policy. A binary's `fn main` is the process boundary: an `Err` reaching it \
             unreported is the equivalent of crashing, not reporting to the user. Add \
             `#[instrument(err(level = \"warn\"))]` (or `err(...)`) to `fn main`, or emit \
             `tracing::warn!`/`tracing::error!` on the error path before returning. \
             `--apply` does not rewrite these rows.\n\n",
        );

        for crate_name in crate_names(&open) {
            let crate_open: Vec<_> = open
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            body.push_str(&format!("## `{crate_name}`\n\n"));

            let mut by_rule: BTreeMap<String, Vec<&BoundaryRow>> = BTreeMap::new();
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
            name: "tracing-boundary.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `tracing-boundary-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct BoundarySummaryReporter;

impl BoundarySummaryReporter {
    pub const ID: &'static str = "tracing-boundary-summary";
}

impl Reporter for BoundarySummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = boundary_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let total = open.len();

        let mut body = String::new();
        body.push_str("# Binary error-boundary summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** binary error-boundary gaps.\n\n"
        ));
        body.push_str("| Crate | Boundary gaps |\n");
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
            name: "tracing-boundary-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
