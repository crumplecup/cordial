use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::Reporter;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};
use crate::session::SessionView;

#[derive(Debug, Default, Clone)]
struct TracingRow {
    crate_name: String,
    qualified_name: String,
    function_kind: String,
    visibility: String,
    file: String,
    line: String,
    disposition: String,
    suppression_reason: String,
}

impl TracingRow {
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
            qualified_name: field("qualified_name"),
            function_kind: field("function_kind"),
            visibility: field("visibility"),
            file: field("file"),
            line: field("line"),
            disposition: finding.disposition().to_string(),
            suppression_reason: field("suppression_reason"),
        }
    }
}

fn tracing_rows(findings: &[&dyn Finding]) -> Vec<TracingRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "tracing")
        .map(|finding| TracingRow::from_finding(*finding))
        .collect()
}

fn open_rows(rows: &[TracingRow]) -> impl Iterator<Item = &TracingRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Writes `tracing-instrument.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct TracingCsvReporter;

impl TracingCsvReporter {
    pub const ID: &'static str = "tracing-csv";
}

impl Reporter for TracingCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let mut body = String::from("crate,qualified_name,kind,visibility,file,line,disposition\n");
        for row in tracing_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                row.crate_name,
                row.qualified_name,
                row.function_kind,
                row.visibility,
                row.file,
                row.line,
                row.disposition,
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "tracing-instrument.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `tracing-instrument.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct TracingChecklistReporter;

impl TracingChecklistReporter {
    pub const ID: &'static str = "tracing-checklist";
}

impl Reporter for TracingChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = tracing_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let suppressed: Vec<_> = rows
            .iter()
            .filter(|row| row.disposition == "suppressed")
            .collect();

        let mut body = String::new();
        body.push_str("# Tracing instrument checklist\n\n");
        body.push_str(&format!("**Open gaps:** {}\n\n", open.len()));
        body.push_str("Add `#[instrument]` (or `#[tracing::instrument]`) to each item below.\n\n");
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        let mut by_module: BTreeMap<String, Vec<&TracingRow>> = BTreeMap::new();
        for row in open {
            let module = module_key(&row.qualified_name);
            by_module.entry(module).or_default().push(row);
        }

        for (module, entries) in by_module {
            if module.is_empty() {
                body.push_str("### crate root\n\n");
            } else {
                body.push_str(&format!("### `{module}`\n\n"));
            }
            for entry in entries {
                body.push_str(&format!(
                    "- [ ] `{}` — `{}:{}` ({})\n",
                    entry.qualified_name, entry.file, entry.line, entry.visibility
                ));
            }
            body.push('\n');
        }

        if !suppressed.is_empty() {
            body.push_str("### Documented exceptions\n\n");
            for entry in suppressed {
                body.push_str(&format!(
                    "- [x] `{}` — `{}:{}` — _{}_\n",
                    entry.qualified_name, entry.file, entry.line, entry.suppression_reason,
                ));
            }
            body.push('\n');
        }

        Ok(vec![Box::new(TextArtifact {
            name: "tracing-instrument.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `tracing-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct TracingSummaryReporter;

impl TracingSummaryReporter {
    pub const ID: &'static str = "tracing-summary";
}

impl Reporter for TracingSummaryReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = tracing_rows(findings);
        let open = open_rows(&rows).count();
        let suppressed = rows
            .iter()
            .filter(|row| row.disposition == "suppressed")
            .count();

        let mut body = String::new();
        body.push_str("# Tracing instrument summary\n\n");
        body.push_str(&format!(
            "Workspace totals: **{open}** open gaps, **{suppressed}** documented exceptions.\n\n"
        ));
        body.push_str("| Crate | Open gaps | Documented exceptions |\n");
        body.push_str("| --- | ---: | ---: |\n");
        body.push_str(&format!(
            "| `{}` | {open} | {suppressed} |\n",
            ir.crate_name()
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "tracing-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

fn module_key(qualified_name: &str) -> String {
    match qualified_name.rsplit_once("::") {
        Some((module, _)) => module.to_string(),
        None => String::new(),
    }
}
