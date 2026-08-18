use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::Reporter;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};
use crate::session::SessionView;

use super::types::FunctionRole;

#[derive(Debug, Default, Clone)]
struct TracingRow {
    crate_name: String,
    qualified_name: String,
    function_kind: String,
    role: String,
    complexity: String,
    rule: String,
    visibility: String,
    recipe: String,
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
            role: field("role"),
            complexity: field("complexity"),
            rule: field("rule"),
            visibility: field("visibility"),
            recipe: field("recipe"),
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

fn crate_names(rows: &[&TracingRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

fn escape_csv(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
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
        let mut body = String::from(
            "crate,qualified_name,role,complexity,rule,function_kind,visibility,recipe,file,line,disposition\n",
        );
        for row in tracing_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{}\n",
                row.crate_name,
                escape_csv(&row.qualified_name),
                row.role,
                row.complexity,
                row.rule,
                row.function_kind,
                row.visibility,
                escape_csv(&row.recipe),
                escape_csv(&row.file),
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
        _ir: &dyn IrView,
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
        body.push_str(
            "Apply writes the listed recipe (`level`, `skip`, `err`, `ret`, `fields`). \
             Do not skip getters — filter at the subscriber. \
             `TRACING-ERROR-PATH-SILENT` only fires when the recipe wants `err` and the body \
             has neither `err` nor `warn!`/`error!`.\n\n",
        );

        for crate_name in crate_names(&open) {
            body.push_str(&format!("## `{crate_name}`\n\n"));
            let crate_rows: Vec<_> = open
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            for role in FunctionRole::ALL {
                let role_rows: Vec<_> = crate_rows
                    .iter()
                    .copied()
                    .filter(|row| row.role == role.as_str())
                    .collect();
                if role_rows.is_empty() {
                    continue;
                }
                body.push_str(&format!("### `{}`\n\n", role.as_str()));
                let mut by_module: BTreeMap<String, Vec<&TracingRow>> = BTreeMap::new();
                for row in role_rows {
                    by_module
                        .entry(module_key(&row.qualified_name))
                        .or_default()
                        .push(row);
                }
                for (module, entries) in by_module {
                    if module.is_empty() {
                        body.push_str("#### crate root\n\n");
                    } else {
                        body.push_str(&format!("#### `{module}`\n\n"));
                    }
                    for entry in entries {
                        body.push_str(&format!(
                            "- [ ] `{}` — `{}:{}` ({}) — `{}` — `{}`\n",
                            entry.qualified_name,
                            entry.file,
                            entry.line,
                            entry.visibility,
                            entry.rule,
                            entry.recipe,
                        ));
                    }
                    body.push('\n');
                }
            }
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
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = tracing_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let suppressed = rows
            .iter()
            .filter(|row| row.disposition == "suppressed")
            .count();

        let mut body = String::new();
        body.push_str("# Tracing instrument summary\n\n");
        body.push_str(&format!(
            "Workspace totals: **{}** open gaps, **{suppressed}** documented exceptions.\n\n",
            open.len()
        ));

        body.push_str("| Crate | Open |");
        for role in FunctionRole::ALL {
            body.push_str(&format!(" {} |", role.as_str()));
        }
        body.push_str(" Exceptions |\n");
        body.push_str("| --- | ---: |");
        for _ in FunctionRole::ALL {
            body.push_str(" ---: |");
        }
        body.push_str(" ---: |\n");

        for crate_name in crate_names(&open) {
            let crate_open: Vec<_> = open
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            let crate_suppressed = rows
                .iter()
                .filter(|row| row.crate_name == crate_name && row.disposition == "suppressed")
                .count();
            body.push_str(&format!("| `{crate_name}` | {} |", crate_open.len()));
            for role in FunctionRole::ALL {
                let count = crate_open
                    .iter()
                    .filter(|row| row.role == role.as_str())
                    .count();
                body.push_str(&format!(" {count} |"));
            }
            body.push_str(&format!(" {crate_suppressed} |\n"));
        }

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
