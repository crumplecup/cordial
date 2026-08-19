//! Version-in-member reporters (CSV, checklist, summary).

use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, TextArtifact};

use super::reporter::{AntipatternRow, antipattern_rows, escape_csv};
use super::types::{AntipatternRuleId, build_workspace_version_in_member_summary};

use tracing::instrument;
#[instrument(level = "debug", skip(findings))]
fn version_rows(findings: &[&dyn Finding]) -> Vec<AntipatternRow> {
    antipattern_rows(findings)
        .into_iter()
        .filter(|row| row.rule_id == AntipatternRuleId::VersionInMember001.as_str())
        .collect()
}

/// Writes `version-in-member.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct VersionInMemberCsvReporter;

impl VersionInMemberCsvReporter {
    pub const ID: &'static str = "version-in-member-csv";
}

impl Reporter for VersionInMemberCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,rule_id,context,file,line,snippet\n");
        for row in version_rows(findings)
            .iter()
            .filter(|row| row.disposition == "open")
        {
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
            name: "version-in-member.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `version-in-member.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct VersionInMemberChecklistReporter;

impl VersionInMemberChecklistReporter {
    pub const ID: &'static str = "version-in-member-checklist";
}

impl Reporter for VersionInMemberChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let version = version_rows(findings);
        let rows: Vec<_> = version
            .iter()
            .filter(|row| row.disposition == "open")
            .collect();
        let mut body = String::new();
        body.push_str("# Version in member checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", rows.len()));
        body.push_str(
            "Workspace members should inherit crate and dependency versions from the root \
             `Cargo.toml` via `*.workspace = true` rather than repeating inline `version = \"…\"` \
             keys, shorthand registry deps (`serde = \"1\"`), or duplicated path deps that already \
             exist in `[workspace.dependencies]`. When a dependency is already declared in the root \
             manifest, use `{{dep}}.workspace = true`; otherwise add it to `[workspace.dependencies]` \
             first.\n\n",
        );

        if !rows.is_empty() {
            body.push_str(&format!("## `{}`\n\n", ir.crate_name()));
            let mut by_rule: BTreeMap<String, Vec<&&AntipatternRow>> = BTreeMap::new();
            for row in &rows {
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
        }

        Ok(vec![Box::new(TextArtifact {
            name: "version-in-member.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `version-in-member-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct VersionInMemberSummaryReporter;

impl VersionInMemberSummaryReporter {
    pub const ID: &'static str = "version-in-member-summary";
}

impl Reporter for VersionInMemberSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let summary = build_workspace_version_in_member_summary(findings);
        let mut body = String::new();
        body.push_str("# Version in member summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{}** inline-version sites across **{}** member crates.\n\n",
            summary.total, summary.crates_with_findings,
        ));
        body.push_str(
            "Action items live in [`version-in-member.checklist.md`](version-in-member.checklist.md).\n\n",
        );

        if !summary.crates.is_empty() {
            body.push_str("## Per crate\n\n");
            body.push_str("| Crate | Open items |\n");
            body.push_str("| --- | ---: |\n");
            for row in &summary.crates {
                body.push_str(&format!("| `{}` | {} |\n", row.crate_name, row.total));
            }
            body.push('\n');
        }

        Ok(vec![Box::new(TextArtifact {
            name: "version-in-member-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
