use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct PanicRow {
    crate_name: String,
    kind: String,
    surface: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    disposition: String,
    suppression_reason: String,
    checklist: String,
}

impl PanicRow {
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
            kind: field("kind"),
            surface: field("surface"),
            context: field("context"),
            file: field("file"),
            line: field("line"),
            snippet: field("snippet"),
            disposition: finding.disposition().to_string(),
            suppression_reason: field("suppression_reason"),
            checklist: field("checklist"),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn panic_rows(findings: &[&dyn Finding]) -> Vec<PanicRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "panics")
        .map(|finding| PanicRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_panic_rows(rows: &[PanicRow]) -> impl Iterator<Item = &PanicRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

#[instrument(level = "debug", skip(rows))]
fn checklist_panic_rows(rows: &[PanicRow]) -> impl Iterator<Item = &PanicRow> {
    open_panic_rows(rows).filter(|row| row.checklist != "false")
}

/// Writes `panics.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PanicCsvReporter;

impl PanicCsvReporter {
    pub const ID: &'static str = "panic-csv";
}

impl Reporter for PanicCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,kind,surface,context,file,line,snippet\n");
        for row in panic_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                row.crate_name,
                row.kind,
                row.surface,
                row.context,
                row.file,
                row.line,
                escape_csv(&row.snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "panics.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `panics.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PanicChecklistReporter;

impl PanicChecklistReporter {
    pub const ID: &'static str = "panic-checklist";
}

impl Reporter for PanicChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = panic_rows(findings);
        let open: Vec<_> = checklist_panic_rows(&rows).collect();
        let inventory = open_panic_rows(&rows).count();
        let mut body = String::new();
        body.push_str("# Panic sources checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Abort sites (`panic!`, `unreachable!`, `.expect(…)`, `.unwrap(…)`, \
             `compile_error!`). Replacement depends on the surface: **library** \
             code should wrap the associated error in the crate's internal type \
             (`From` / `map_err` / `?`, preserving `source()`); **binary** and \
             **test** code should surface failures with miette.\n\n",
        );
        if inventory > open.len() {
            body.push_str(&format!(
                "_{} inventory rows in `panics.csv`._\n\n",
                inventory
            ));
        }
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        let mut by_surface: BTreeMap<String, Vec<&PanicRow>> = BTreeMap::new();
        for row in &open {
            let surface = if row.surface.is_empty() {
                "library".to_string()
            } else {
                row.surface.clone()
            };
            by_surface.entry(surface).or_default().push(row);
        }

        for (surface, entries) in by_surface {
            let (title, action) = match surface.as_str() {
                "binary" => (
                    "Binary — surface with miette",
                    crate::plugin::ErrorSurface::Binary.abort_action(),
                ),
                "test" => (
                    "Tests — surface with miette",
                    crate::plugin::ErrorSurface::Test.abort_action(),
                ),
                _ => (
                    "Library — return internal error types",
                    crate::plugin::ErrorSurface::Library.abort_action(),
                ),
            };
            body.push_str(&format!("### {title}\n\n"));
            body.push_str(&format!("_{action}._\n\n"));
            let mut by_kind: BTreeMap<String, Vec<&PanicRow>> = BTreeMap::new();
            for entry in entries {
                by_kind.entry(entry.kind.clone()).or_default().push(entry);
            }
            for (kind, kind_entries) in by_kind {
                body.push_str(&format!("#### {kind}\n\n"));
                for entry in kind_entries {
                    body.push_str(&format!(
                        "- [ ] `{}` — `{}:{}` — `{}`\n",
                        entry.context, entry.file, entry.line, entry.snippet
                    ));
                }
                body.push('\n');
            }
        }

        let suppressed: Vec<_> = rows
            .iter()
            .filter(|row| row.disposition == "suppressed")
            .collect();
        if !suppressed.is_empty() {
            body.push_str("### Documented exceptions\n\n");
            for entry in suppressed {
                body.push_str(&format!(
                    "- [x] `{}` — `{}:{}` — _{}_\n",
                    entry.context, entry.file, entry.line, entry.suppression_reason
                ));
            }
            body.push('\n');
        }

        Ok(vec![Box::new(TextArtifact {
            name: "panics.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `panics-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct PanicSummaryReporter;

impl PanicSummaryReporter {
    pub const ID: &'static str = "panic-summary";
}

impl Reporter for PanicSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = panic_rows(findings);
        let inventory: Vec<_> = open_panic_rows(&rows).collect();
        let open: Vec<_> = checklist_panic_rows(&rows).collect();
        let mut panic = 0usize;
        let mut unreachable = 0usize;
        let mut expect = 0usize;
        let mut unwrap = 0usize;
        let mut compile_error = 0usize;

        for row in &open {
            match row.kind.as_str() {
                "PANIC-SOURCE-PANIC" => panic += 1,
                "PANIC-SOURCE-UNREACHABLE" => unreachable += 1,
                "PANIC-SOURCE-EXPECT" => expect += 1,
                "PANIC-SOURCE-UNWRAP" => unwrap += 1,
                "PANIC-SOURCE-COMPILE-ERROR" => compile_error += 1,
                _ => {}
            }
        }

        let mut library = 0usize;
        let mut binary = 0usize;
        let mut test = 0usize;
        for row in &open {
            match row.surface.as_str() {
                "binary" => binary += 1,
                "test" => test += 1,
                _ => library += 1,
            }
        }

        let total = open.len();
        let inventory_total = inventory.len();
        let mut body = String::new();
        body.push_str("# Panic sources summary\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** abort-site action items \
             (**{inventory_total}** rows in `panics.csv`) — panic **{panic}**, unreachable **{unreachable}**, \
             expect **{expect}**, unwrap **{unwrap}**, compile_error **{compile_error}**. \
             Library **{library}** (internal error types), binary **{binary}** (miette), \
             tests **{test}** (miette).\n\n"
        ));
        body.push_str(
            "| Crate | Total | Panic | Unreachable | Expect | Unwrap | Compile error |\n",
        );
        body.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
        body.push_str(&format!(
            "| `{}` | {total} | {panic} | {unreachable} | {expect} | {unwrap} | {compile_error} |\n",
            ir.crate_name()
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "panics-summary.md".to_string(),
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
