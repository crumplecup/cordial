use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;

#[derive(Debug, Default, Clone)]
struct GlobImportRow {
    crate_name: String,
    rule_id: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
}

impl GlobImportRow {
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
fn glob_import_rows(findings: &[&dyn Finding]) -> Vec<GlobImportRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "glob_imports")
        .map(|finding| GlobImportRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[GlobImportRow]) -> impl Iterator<Item = &GlobImportRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Writes `glob-imports.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct GlobImportCsvReporter;

impl GlobImportCsvReporter {
    pub const ID: &'static str = "glob-import-csv";
}

impl Reporter for GlobImportCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,rule_id,context,file,line,snippet\n");
        for row in glob_import_rows(findings) {
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
            name: "glob-imports.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `glob-imports.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct GlobImportChecklistReporter;

impl GlobImportChecklistReporter {
    pub const ID: &'static str = "glob-import-checklist";
}

impl Reporter for GlobImportChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = glob_import_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# Glob imports checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Replace each glob `use` with explicit names so readers and IDEs \
             can see the surface a file depends on. That includes `use super::*;`.\n\n",
        );
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        let mut by_rule: BTreeMap<String, Vec<&GlobImportRow>> = BTreeMap::new();
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
            name: "glob-imports.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `glob-imports-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct GlobImportSummaryReporter;

impl GlobImportSummaryReporter {
    pub const ID: &'static str = "glob-import-summary";
}

impl Reporter for GlobImportSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = glob_import_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let total = open.len();

        let mut body = String::new();
        body.push_str("# Glob imports summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** glob `use` sites.\n\n"
        ));
        body.push_str("| Crate | Glob imports |\n");
        body.push_str("| --- | ---: |\n");
        body.push_str(&format!("| `{}` | {total} |\n", ir.crate_name()));
        body.push_str(&format!("\n| **Total** | **{total}** |\n"));

        Ok(vec![Box::new(TextArtifact {
            name: "glob-imports-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
