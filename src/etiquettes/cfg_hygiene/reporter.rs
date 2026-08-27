use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct CfgHygieneRow {
    crate_name: String,
    rule_id: String,
    cfg_name: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl CfgHygieneRow {
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
            cfg_name: field("cfg_name"),
            context: field("context"),
            file: field("file"),
            line: field("line"),
            snippet: field("snippet"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn cfg_hygiene_rows(findings: &[&dyn Finding]) -> Vec<CfgHygieneRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "cfg_hygiene")
        .map(|finding| CfgHygieneRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[CfgHygieneRow]) -> impl Iterator<Item = &CfgHygieneRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

#[instrument(level = "debug", skip(rows))]
fn sort_by_file_line(rows: &mut [&CfgHygieneRow]) {
    rows.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then_with(|| {
                left.line
                    .parse::<u32>()
                    .unwrap_or(0)
                    .cmp(&right.line.parse::<u32>().unwrap_or(0))
            })
            .then_with(|| left.cfg_name.cmp(&right.cfg_name))
    });
}

/// Writes `cfg-hygiene.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgHygieneCsvReporter;

impl CfgHygieneCsvReporter {
    pub const ID: &'static str = "cfg-hygiene-csv";
}

impl Reporter for CfgHygieneCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let all_rows = cfg_hygiene_rows(findings);
        let mut rows: Vec<_> = open_rows(&all_rows).collect();
        sort_by_file_line(&mut rows);

        let mut body = String::from("crate,rule_id,cfg_name,context,file,line,snippet\n");
        for row in rows {
            body.push_str(&format!(
                "{},{},{},\"{}\",{},{},\"{}\"\n",
                row.crate_name,
                row.rule_id,
                row.cfg_name,
                row.context.replace('"', "''"),
                row.file,
                row.line,
                row.snippet.replace('"', "''"),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "cfg-hygiene.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `cfg-hygiene.checklist.md`, grouped by the crate each finding
/// actually belongs to (from its own `crate` field) — not the crate
/// currently being rendered, since `view.findings` accumulates every
/// crate's findings across a whole workspace run.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgHygieneChecklistReporter;

impl CfgHygieneChecklistReporter {
    pub const ID: &'static str = "cfg-hygiene-checklist";
}

impl Reporter for CfgHygieneChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = cfg_hygiene_rows(findings);
        let mut open: Vec<_> = open_rows(&rows).collect();
        sort_by_file_line(&mut open);

        let mut body = String::new();
        body.push_str("# cfg hygiene checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "`UNEXPECTED-CFG-001`: a `cfg`/`cfg_attr` name not declared in any check-cfg \
             source reachable by that crate. `CFG-VERIFIER-MISMATCH-001`: a crate registered \
             in `cordial.toml`'s `[cfg_hygiene] crate_verifier` table using a different \
             verifier's cfg name than its own configured identity.\n\n",
        );

        if open.is_empty() {
            body.push_str("_No cfg-hygiene findings._\n\n");
        } else {
            let mut by_crate: BTreeMap<&str, Vec<&&CfgHygieneRow>> = BTreeMap::new();
            for row in &open {
                by_crate
                    .entry(row.crate_name.as_str())
                    .or_default()
                    .push(row);
            }
            for (crate_name, crate_rows) in by_crate {
                body.push_str(&format!("## `{crate_name}`\n\n"));
                for row in crate_rows {
                    body.push_str(&format!(
                        "- [ ] `{}` — `{}` — `cfg({})` at `{}` (line {})\n  - {}\n",
                        row.rule_id, row.context, row.cfg_name, row.file, row.line, row.snippet
                    ));
                }
                body.push('\n');
            }
        }

        Ok(vec![Box::new(TextArtifact {
            name: "cfg-hygiene.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Per-crate rollup row.
#[derive(Debug, Clone, Default)]
struct CfgHygieneCrateSummary {
    crate_name: String,
    unexpected: usize,
    verifier_mismatch: usize,
}

/// Writes `cfg-hygiene-summary.md`, one row per crate that has any
/// findings — grouped from each finding's own `crate` field the same way
/// [`CfgHygieneChecklistReporter`] is, not from the crate currently being
/// rendered.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgHygieneSummaryReporter;

impl CfgHygieneSummaryReporter {
    pub const ID: &'static str = "cfg-hygiene-summary";
}

impl Reporter for CfgHygieneSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = cfg_hygiene_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();

        let mut by_crate: BTreeMap<String, CfgHygieneCrateSummary> = BTreeMap::new();
        for row in &open {
            let entry =
                by_crate
                    .entry(row.crate_name.clone())
                    .or_insert_with(|| CfgHygieneCrateSummary {
                        crate_name: row.crate_name.clone(),
                        unexpected: 0,
                        verifier_mismatch: 0,
                    });
            if row.rule_id == "UNEXPECTED-CFG-001" {
                entry.unexpected += 1;
            } else if row.rule_id == "CFG-VERIFIER-MISMATCH-001" {
                entry.verifier_mismatch += 1;
            }
        }

        let total_unexpected: usize = by_crate.values().map(|c| c.unexpected).sum();
        let total_verifier_mismatch: usize = by_crate.values().map(|c| c.verifier_mismatch).sum();

        let mut body = String::new();
        body.push_str("# cfg hygiene summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total_unexpected}** undeclared cfg names, \
             **{total_verifier_mismatch}** verifier cfg mismatches.\n\n",
        ));
        body.push_str("| Crate | Undeclared cfg names | Verifier mismatches |\n");
        body.push_str("| --- | ---: | ---: |\n");
        for summary in by_crate.values() {
            body.push_str(&format!(
                "| `{}` | {} | {} |\n",
                summary.crate_name, summary.unexpected, summary.verifier_mismatch
            ));
        }

        Ok(vec![Box::new(TextArtifact {
            name: "cfg-hygiene-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
