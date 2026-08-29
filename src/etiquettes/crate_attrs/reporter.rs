use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;

#[derive(Debug, Default, Clone)]
struct CrateAttrsRow {
    crate_name: String,
    rule_id: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl CrateAttrsRow {
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
            file: field("file"),
            line: field("line"),
            snippet: field("snippet"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn crate_attrs_rows(findings: &[&dyn Finding]) -> Vec<CrateAttrsRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "crate_attrs")
        .map(|finding| CrateAttrsRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[CrateAttrsRow]) -> impl Iterator<Item = &CrateAttrsRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

#[instrument(level = "debug", skip(rows))]
fn sort_by_crate_rule(rows: &mut [&CrateAttrsRow]) {
    rows.sort_by(|left, right| {
        left.crate_name
            .cmp(&right.crate_name)
            .then_with(|| left.rule_id.cmp(&right.rule_id))
            .then_with(|| left.file.cmp(&right.file))
    });
}

/// Writes `crate-attrs.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CrateAttrsCsvReporter;

impl CrateAttrsCsvReporter {
    pub const ID: &'static str = "crate-attrs-csv";
}

impl Reporter for CrateAttrsCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let all_rows = crate_attrs_rows(findings);
        let mut rows: Vec<_> = open_rows(&all_rows).collect();
        sort_by_crate_rule(&mut rows);

        let mut body = String::from("crate,rule_id,file,line,snippet\n");
        for row in rows {
            body.push_str(&format!(
                "{},{},{},{},{}\n",
                row.crate_name,
                row.rule_id,
                row.file,
                row.line,
                escape_csv(&row.snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "crate-attrs.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `crate-attrs.checklist.md`, grouped by the crate each finding
/// belongs to (from its own `crate` field).
#[derive(Debug, Default, Clone, Copy)]
pub struct CrateAttrsChecklistReporter;

impl CrateAttrsChecklistReporter {
    pub const ID: &'static str = "crate-attrs-checklist";
}

impl Reporter for CrateAttrsChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = crate_attrs_rows(findings);
        let mut open: Vec<_> = open_rows(&rows).collect();
        sort_by_crate_rule(&mut open);

        let mut body = String::new();
        body.push_str("# Crate attributes checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "`CRATE-FORBID-UNSAFE-001`: library root is missing `#![forbid(unsafe_code)]`. \
             `CRATE-MISSING-DOCS-001`: library root is missing `#![warn(missing_docs)]` \
             (`deny`/`forbid` also count). Bin-only packages are skipped. \
             `[crate_attrs] allow_unsafe` / `allow_missing_docs` list member exceptions.\n\n",
        );

        if open.is_empty() {
            body.push_str("_No crate-attribute findings._\n\n");
        } else {
            let mut by_crate: BTreeMap<&str, Vec<&&CrateAttrsRow>> = BTreeMap::new();
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
                        "- [ ] `{}` — `{}` (line {})\n  - {}\n",
                        row.rule_id, row.file, row.line, row.snippet
                    ));
                }
                body.push('\n');
            }
        }

        Ok(vec![Box::new(TextArtifact {
            name: "crate-attrs.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

#[derive(Debug, Clone, Default)]
struct CrateAttrsCrateSummary {
    crate_name: String,
    forbid_unsafe: usize,
    missing_docs: usize,
}

/// Writes `crate-attrs-summary.md`, one row per crate that is missing a
/// declaration.
#[derive(Debug, Default, Clone, Copy)]
pub struct CrateAttrsSummaryReporter;

impl CrateAttrsSummaryReporter {
    pub const ID: &'static str = "crate-attrs-summary";
}

impl Reporter for CrateAttrsSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = crate_attrs_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();

        let mut by_crate: BTreeMap<String, CrateAttrsCrateSummary> = BTreeMap::new();
        for row in &open {
            let entry =
                by_crate
                    .entry(row.crate_name.clone())
                    .or_insert_with(|| CrateAttrsCrateSummary {
                        crate_name: row.crate_name.clone(),
                        forbid_unsafe: 0,
                        missing_docs: 0,
                    });
            if row.rule_id == "CRATE-FORBID-UNSAFE-001" {
                entry.forbid_unsafe += 1;
            } else if row.rule_id == "CRATE-MISSING-DOCS-001" {
                entry.missing_docs += 1;
            }
        }

        let total_unsafe: usize = by_crate.values().map(|c| c.forbid_unsafe).sum();
        let total_docs: usize = by_crate.values().map(|c| c.missing_docs).sum();

        let mut body = String::new();
        body.push_str("# Crate attributes summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total_unsafe}** missing `forbid(unsafe_code)`, \
             **{total_docs}** missing `warn(missing_docs)`.\n\n",
        ));
        body.push_str("| Crate | Missing `forbid(unsafe_code)` | Missing `warn(missing_docs)` |\n");
        body.push_str("| --- | ---: | ---: |\n");
        for summary in by_crate.values() {
            body.push_str(&format!(
                "| `{}` | {} | {} |\n",
                summary.crate_name, summary.forbid_unsafe, summary.missing_docs
            ));
        }

        Ok(vec![Box::new(TextArtifact {
            name: "crate-attrs-summary.md".to_string(),
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
