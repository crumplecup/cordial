use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;

#[derive(Debug, Default, Clone)]
struct FeatureWarningRow {
    crate_name: String,
    rule_id: String,
    lint: String,
    file: String,
    line: String,
    message: String,
    gate: String,
    wide: bool,
    advice: String,
    triggering: String,
    disposition: String,
}

impl FeatureWarningRow {
    #[instrument(level = "debug", skip(finding), ret)]
    fn from_finding(finding: &dyn Finding) -> Self {
        let mut sink = MapFindingSink::default();
        finding.emit(&mut sink);
        let field = |name: &str| {
            sink.fields()
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        Self {
            crate_name: field("crate"),
            rule_id: field("rule_id"),
            lint: field("lint"),
            file: field("file"),
            line: field("line"),
            message: field("message"),
            gate: field("gate"),
            wide: field("wide") == "true",
            advice: field("advice"),
            triggering: field("triggering"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn warning_rows(findings: &[&dyn Finding]) -> Vec<FeatureWarningRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "feature_warnings")
        .map(|finding| FeatureWarningRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[FeatureWarningRow]) -> impl Iterator<Item = &FeatureWarningRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Distinct crate names present in `rows`, sorted (a workspace-spanning
/// artifact derives its breakdown from the rows, not the run's first crate).
#[instrument(level = "debug", skip(rows))]
fn crate_names(rows: &[&FeatureWarningRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

/// Checklist heading for a gate. A gate wider than the private-feature
/// threshold is shown by count and its first few names; the full predicate
/// stays in the CSV.
#[instrument(level = "trace")]
fn gate_heading(gate: &str, wide: bool) -> String {
    if gate.is_empty() {
        return "No single gate".to_string();
    }
    if !wide {
        return format!("`#[cfg({gate})]`");
    }
    let names: Vec<&str> = gate
        .split("feature = \"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect();
    let kind = if gate.starts_with("all(") {
        "all"
    } else {
        "any"
    };
    format!(
        "Needs a private feature: {} features ({kind} of {})",
        names.len(),
        names
            .iter()
            .take(3)
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
            + ", ..."
    )
}

/// Writes `feature-warnings.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct FeatureWarningCsvReporter;

impl FeatureWarningCsvReporter {
    /// Stable identifier for `FeatureWarningCsvReporter`.
    pub const ID: &'static str = "feature-warning-csv";
}

impl Reporter for FeatureWarningCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let mut body =
            String::from("crate,rule_id,lint,file,line,gate,triggering,message,advice\n");
        for row in warning_rows(view.findings()) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.rule_id),
                csv_field(&row.lint),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.gate),
                csv_field(&row.triggering),
                csv_field(&row.message),
                csv_field(&row.advice),
            ));
        }
        Ok(vec![Box::new(TextArtifact::new(
            "feature-warnings.csv".to_string(),
            "text/csv".to_string(),
            body,
        ))])
    }
}

/// Writes `feature-warnings.checklist.md`, grouped by suggested gate so one
/// `cfg` edit clears a whole cluster.
#[derive(Debug, Default, Clone, Copy)]
pub struct FeatureWarningChecklistReporter;

impl FeatureWarningChecklistReporter {
    /// Stable identifier for `FeatureWarningChecklistReporter`.
    pub const ID: &'static str = "feature-warning-checklist";
}

impl Reporter for FeatureWarningChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = warning_rows(view.findings());
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# feature warnings checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "`cargo check` and clippy compile one feature set. These problems \
             appear only under some feature combinations of `cargo hack check \
             --feature-powerset`, so they stay hidden until another feature \
             set is built. Combinations that do not compile come first; the \
             warnings are grouped by the `cfg` gate that would silence them.\n\n",
        );

        for crate_name in crate_names(&open) {
            let crate_open: Vec<_> = open
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            body.push_str(&format!("## `{crate_name}`\n\n"));

            let (failures, warnings): (Vec<&FeatureWarningRow>, Vec<&FeatureWarningRow>) =
                crate_open
                    .iter()
                    .copied()
                    .partition(|row| row.rule_id == "FEATURE-WARNING-003");

            // Failures lead: a combination that does not compile cannot have
            // its warnings assessed, so it is the more serious problem.
            let mut by_code: BTreeMap<String, Vec<&FeatureWarningRow>> = BTreeMap::new();
            for row in &failures {
                by_code.entry(row.lint.clone()).or_default().push(row);
            }
            for (code, entries) in by_code {
                body.push_str(&format!(
                    "### Does not compile: `{code}` ({})\n\n{}\n\n",
                    entries.len(),
                    entries[0].advice
                ));
                for entry in entries {
                    body.push_str(&format!(
                        "- [ ] `{}:{}` — {} ({})\n",
                        entry.file, entry.line, entry.message, entry.triggering
                    ));
                }
                body.push('\n');
            }

            let mut by_gate: BTreeMap<String, Vec<&FeatureWarningRow>> = BTreeMap::new();
            for row in &warnings {
                by_gate.entry(row.gate.clone()).or_default().push(row);
            }

            for (gate, entries) in by_gate {
                body.push_str(&format!(
                    "### {} ({})\n\n{}\n\n",
                    gate_heading(&gate, entries[0].wide),
                    entries.len(),
                    entries[0].advice
                ));
                for entry in entries {
                    body.push_str(&format!(
                        "- [ ] `{}:{}` — {} (`{}`; {})\n",
                        entry.file, entry.line, entry.message, entry.lint, entry.triggering
                    ));
                }
                body.push('\n');
            }
        }

        Ok(vec![Box::new(TextArtifact::new(
            "feature-warnings.checklist.md".to_string(),
            "text/markdown".to_string(),
            body,
        ))])
    }
}

/// Writes `feature-warnings-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct FeatureWarningSummaryReporter;

impl FeatureWarningSummaryReporter {
    /// Stable identifier for `FeatureWarningSummaryReporter`.
    pub const ID: &'static str = "feature-warning-summary";
}

impl Reporter for FeatureWarningSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = warning_rows(view.findings());
        let open: Vec<_> = open_rows(&rows).collect();
        let total = open.len();

        let mut body = String::new();
        body.push_str("# feature warnings summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** feature-dependent warnings.\n\n"
        ));
        body.push_str("| Crate | feature warnings |\n");
        body.push_str("| --- | ---: |\n");
        for crate_name in crate_names(&open) {
            let crate_total = open
                .iter()
                .filter(|row| row.crate_name == crate_name)
                .count();
            body.push_str(&format!("| `{crate_name}` | {crate_total} |\n"));
        }
        body.push_str(&format!("\n| **Total** | **{total}** |\n"));

        Ok(vec![Box::new(TextArtifact::new(
            "feature-warnings-summary.md".to_string(),
            "text/markdown".to_string(),
            body,
        ))])
    }
}
