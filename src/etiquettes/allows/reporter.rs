use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct AllowRow {
    crate_name: String,
    rule_id: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl AllowRow {
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
fn allow_rows(findings: &[&dyn Finding]) -> Vec<AllowRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "allows")
        .map(|finding| AllowRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[AllowRow]) -> impl Iterator<Item = &AllowRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Writes `allows.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowCsvReporter;

impl AllowCsvReporter {
    pub const ID: &'static str = "allow-csv";
}

impl Reporter for AllowCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,rule_id,context,file,line,snippet\n");
        for row in allow_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{}\n",
                row.crate_name,
                row.rule_id,
                row.context,
                row.file,
                row.line,
                escape_csv(&row.snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "allows.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `allows.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowChecklistReporter;

impl AllowChecklistReporter {
    pub const ID: &'static str = "allow-checklist";
}

impl Reporter for AllowChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = allow_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# Allow attributes checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Inventory of `#[allow(...)]` and `#![allow(...)]` attributes in crate \
             `src/` and `tests/` trees. Project policy: fix root causes instead of \
             suppressing rustc or clippy warnings. Verus `vstd` / `verus_builtin` \
             imports that are unused under plain rustc must carry \
             `reason = \"...\"`; a reasoned Verus allow is not an action item.\n\n",
        );
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        let mut by_rule: BTreeMap<String, Vec<&AllowRow>> = BTreeMap::new();
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
            name: "allows.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `allows-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowSummaryReporter;

impl AllowSummaryReporter {
    pub const ID: &'static str = "allow-summary";
}

impl Reporter for AllowSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = allow_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let total = open.len();
        let attr = total;

        let mut body = String::new();
        body.push_str("# Allow attributes summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** sites — allow attributes **{attr}**.\n\n"
        ));
        body.push_str("| Crate | Total | Allow attributes |\n");
        body.push_str("| --- | ---: | ---: |\n");
        body.push_str(&format!("| `{}` | {total} | {attr} |\n", ir.crate_name()));
        body.push_str(&format!("\n| **Total** | **{total}** | **{attr}** |\n"));

        Ok(vec![Box::new(TextArtifact {
            name: "allows-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

#[instrument(level = "debug")]
fn escape_csv(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
