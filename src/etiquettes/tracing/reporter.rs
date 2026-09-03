use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use super::types::FunctionRole;

use tracing::instrument;
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

#[instrument(level = "debug", skip(findings))]
fn tracing_rows(findings: &[&dyn Finding]) -> Vec<TracingRow> {
    findings
        .iter()
        .filter(|finding| {
            finding.rule().category() == "tracing"
                && !super::subscriber::SubscriberRuleId::is_subscriber_rule(finding.rule().id())
        })
        .map(|finding| TracingRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[TracingRow]) -> impl Iterator<Item = &TracingRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

#[instrument(level = "debug", skip(rows))]
fn crate_names(rows: &[&TracingRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
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

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from(
            "crate,qualified_name,role,complexity,rule,function_kind,visibility,recipe,file,line,disposition\n",
        );
        for row in tracing_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.qualified_name),
                csv_field(&row.role),
                csv_field(&row.complexity),
                csv_field(&row.rule),
                csv_field(&row.function_kind),
                csv_field(&row.visibility),
                csv_field(&row.recipe),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.disposition),
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

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

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
            "Apply writes the listed recipe (`level`, `skip`, `err`, `ret`, `fields`) \
             for missing/delta rows. `TRACING-PROOF-INSTRUMENT` and \
             `TRACING-SKIP-INSTRUMENT` mean **remove** `#[instrument]` (including a \
             `not(<gate>)` wrap on proof-only code — that span never fires). \
             `TRACING-UNGATED-INSTRUMENT` means wrap with `cfg_attr(not(<gate>), …)`. \
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
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

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

#[instrument(level = "debug")]
fn module_key(qualified_name: &str) -> String {
    let Some((module, _)) = qualified_name.rsplit_once("::") else {
        return String::new();
    };
    // Trait impl methods are recorded as `mod::path::<Ty as Trait>::method`;
    // group them under `mod::path`, not one heading per impl.
    let module = match module.rsplit_once("::<") {
        Some((outer, _)) => outer,
        None => module,
    };
    if module.starts_with('<') {
        // A crate-root trait impl (`<Ty as Trait>::method`) -- no module.
        return String::new();
    }
    module.to_string()
}
