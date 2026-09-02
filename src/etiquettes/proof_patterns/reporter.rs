use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;

#[derive(Debug, Default, Clone)]
struct ProofPatternRow {
    crate_name: String,
    kind: String,
    context: String,
    file: String,
    line: String,
    snippet: String,
    cfg_test: String,
    tracked_params: String,
    recommends: String,
    disposition: String,
}

impl ProofPatternRow {
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
            context: field("context"),
            file: field("file"),
            line: field("line"),
            snippet: field("snippet"),
            cfg_test: field("cfg_test"),
            tracked_params: field("tracked_params"),
            recommends: field("recommends"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn pattern_rows(findings: &[&dyn Finding]) -> Vec<ProofPatternRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "proof_patterns")
        .map(|finding| ProofPatternRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[ProofPatternRow]) -> impl Iterator<Item = &ProofPatternRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Distinct crate names present in `rows`, sorted -- `view.ir.crate_name()`
/// is pinned to whichever crate the run's target discovery lists first, not
/// the crate a given row actually belongs to, so a workspace-spanning
/// artifact must derive its own crate breakdown from `row.crate_name`
/// instead (the same pattern `modularity::reporter::rows::crate_names` uses).
#[instrument(level = "debug", skip(rows))]
fn crate_names(rows: &[&ProofPatternRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

/// Writes `proof-patterns.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProofPatternCsvReporter;

impl ProofPatternCsvReporter {
    pub const ID: &'static str = "proof-pattern-csv";
}

impl Reporter for ProofPatternCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from(
            "crate,kind,context,file,line,snippet,cfg_test,tracked_params,recommends\n",
        );
        for row in pattern_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.kind),
                csv_field(&row.context),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.snippet),
                csv_field(&row.cfg_test),
                csv_field(&row.tracked_params),
                csv_field(&row.recommends),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "proof-patterns.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `proof-patterns.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProofPatternChecklistReporter;

impl ProofPatternChecklistReporter {
    pub const ID: &'static str = "proof-pattern-checklist";
}

impl Reporter for ProofPatternChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = pattern_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# Proof patterns checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Each site is a real, local soundness or proof-visibility signal a real \
             `verus_syn` parse found inside a `verus! { .. }` block: a function that's \
             trusted rather than proven (`assume`/`admit`/`external_body`/`uninterp`/\
             `axiom`), or a `broadcast` lemma applying itself invisibly to every proof \
             in scope.\n\n",
        );

        for crate_name in crate_names(&open) {
            let crate_open: Vec<_> = open
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            body.push_str(&format!("## `{crate_name}`\n\n"));

            let mut by_kind: BTreeMap<String, Vec<&ProofPatternRow>> = BTreeMap::new();
            for row in &crate_open {
                by_kind.entry(row.kind.clone()).or_default().push(row);
            }

            for (kind, entries) in by_kind {
                body.push_str(&format!("### {kind}\n\n"));
                for entry in entries {
                    body.push_str(&format!(
                        "- [ ] `{}` — `{}:{}` — `{}`",
                        entry.context, entry.file, entry.line, entry.snippet
                    ));
                    if !entry.tracked_params.is_empty() {
                        body.push_str(&format!(" — tracked: {}", entry.tracked_params));
                    }
                    if !entry.recommends.is_empty() {
                        body.push_str(&format!(" — recommends: {}", entry.recommends));
                    }
                    if entry.cfg_test == "true" {
                        body.push_str(" — `#[cfg(test)]`");
                    }
                    body.push('\n');
                }
                body.push('\n');
            }
        }

        Ok(vec![Box::new(TextArtifact {
            name: "proof-patterns.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `proof-patterns-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProofPatternSummaryReporter;

impl ProofPatternSummaryReporter {
    pub const ID: &'static str = "proof-pattern-summary";
}

impl Reporter for ProofPatternSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = pattern_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let total = open.len();
        let trusted = open
            .iter()
            .filter(|row| row.kind != "PROOF-PATTERN-BROADCAST")
            .count();
        let broadcasts = total - trusted;

        let mut body = String::new();
        body.push_str("# Proof patterns summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** proof-pattern sites -- **{trusted}** trusted-not-proven \
             (assume/admit/external_body/uninterp/axiom), **{broadcasts}** broadcast lemmas.\n\n"
        ));
        body.push_str("| Crate | Trusted-not-proven | Broadcast | Total |\n");
        body.push_str("| --- | ---: | ---: | ---: |\n");
        for crate_name in crate_names(&open) {
            let crate_open: Vec<_> = open
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            let crate_total = crate_open.len();
            let crate_trusted = crate_open
                .iter()
                .filter(|row| row.kind != "PROOF-PATTERN-BROADCAST")
                .count();
            let crate_broadcasts = crate_total - crate_trusted;
            body.push_str(&format!(
                "| `{crate_name}` | {crate_trusted} | {crate_broadcasts} | {crate_total} |\n"
            ));
        }
        body.push_str(&format!(
            "\n| **Total** | **{trusted}** | **{broadcasts}** | **{total}** |\n"
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "proof-patterns-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
