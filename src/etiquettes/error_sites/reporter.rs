use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use super::types::{ErrorOriginClass, ErrorOriginClassCounts, ErrorSiteKind, ErrorSiteKindCounts};

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct ErrorSiteRow {
    crate_name: String,
    site_kind: String,
    context: String,
    file: String,
    line: String,
    source_snippet: String,
    site_snippet: String,
    origin_class: String,
    origin_detail: String,
    rationale: String,
    disposition: String,
}

impl ErrorSiteRow {
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
            site_kind: field("site_kind"),
            context: field("context"),
            file: field("file"),
            line: field("line"),
            source_snippet: field("source_snippet"),
            site_snippet: field("site_snippet"),
            origin_class: field("origin_class"),
            origin_detail: field("origin_detail"),
            rationale: field("rationale"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn error_site_rows(findings: &[&dyn Finding]) -> Vec<ErrorSiteRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "error_sites")
        .map(|finding| ErrorSiteRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[ErrorSiteRow]) -> impl Iterator<Item = &ErrorSiteRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

#[instrument(level = "debug", skip(rows))]
fn kind_counts(rows: &[ErrorSiteRow]) -> ErrorSiteKindCounts {
    let mut counts = ErrorSiteKindCounts::default();
    for row in rows {
        match row.site_kind.as_str() {
            s if s == ErrorSiteKind::QuestionMark.to_string() => counts.question_mark += 1,
            s if s == ErrorSiteKind::MapErr.to_string() => counts.map_err += 1,
            s if s == ErrorSiteKind::ReturnErr.to_string() => counts.return_err += 1,
            s if s == ErrorSiteKind::IfLetErr.to_string() => counts.if_let_err += 1,
            s if s == ErrorSiteKind::MatchErr.to_string() => counts.match_err += 1,
            s if s == ErrorSiteKind::OkOr.to_string() => counts.ok_or += 1,
            _ => {}
        }
    }
    counts
}

/// Distinct crate names present in `rows`, sorted -- `view.ir.crate_name()`
/// is pinned to whichever crate the run's target discovery lists first, not
/// the crate a given row actually belongs to, so a workspace-spanning
/// artifact must derive its own crate breakdown from `row.crate_name`
/// instead (the same pattern `modularity::reporter::rows::crate_names` uses).
#[instrument(level = "debug", skip(rows))]
fn crate_names(rows: &[&ErrorSiteRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

#[instrument(level = "debug", skip(rows))]
fn origin_counts(rows: &[ErrorSiteRow]) -> ErrorOriginClassCounts {
    let mut counts = ErrorOriginClassCounts::default();
    for row in rows {
        match row.origin_class.as_str() {
            s if s == ErrorOriginClass::Internal.to_string() => counts.internal += 1,
            s if s == ErrorOriginClass::Other.to_string() => counts.other += 1,
            s if s == ErrorOriginClass::Edge.to_string() => counts.edge += 1,
            _ => {}
        }
    }
    counts
}

/// Writes `error-sites.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorSitesCsvReporter;

impl ErrorSitesCsvReporter {
    pub const ID: &'static str = "error-sites-csv";
}

impl Reporter for ErrorSitesCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body =
            String::from("crate,site_kind,context,file,line,source_snippet,site_snippet\n");
        for row in error_site_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.site_kind),
                csv_field(&row.context),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.source_snippet),
                csv_field(&row.site_snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "error-sites.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `error-sites.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorSitesChecklistReporter;

impl ErrorSitesChecklistReporter {
    pub const ID: &'static str = "error-sites-checklist";
}

impl Reporter for ErrorSitesChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = error_site_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# Error sites checklist (intermediate)\n\n");
        body.push_str(&format!("**Sites:** {}\n\n", open.len()));
        body.push_str(
            "Inventory of control-flow sites where a `Result` error is propagated, converted, \
             returned, or constructed. This is an **intermediate** artifact: a follow-up pass \
             partitions rows into **internal** (`CordialError` / crate-local) vs **other** \
             (std, third-party, unresolved). Resolution strategies are out of scope.\n\n",
        );

        for crate_name in crate_names(&open) {
            let crate_open: Vec<_> = open
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            body.push_str(&format!("## `{crate_name}`\n\n"));

            let mut by_kind: BTreeMap<String, Vec<&ErrorSiteRow>> = BTreeMap::new();
            for row in &crate_open {
                by_kind.entry(row.site_kind.clone()).or_default().push(row);
            }

            for (kind, entries) in by_kind {
                body.push_str(&format!("### {kind}\n\n"));
                for entry in entries {
                    body.push_str(&format!(
                        "- [ ] `{}` — `{}:{}` — source `{}` — `{}`\n",
                        entry.context,
                        entry.file,
                        entry.line,
                        entry.source_snippet,
                        entry.site_snippet,
                    ));
                }
                body.push('\n');
            }
        }

        Ok(vec![Box::new(TextArtifact {
            name: "error-sites.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `error-sites-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorSitesSummaryReporter;

impl ErrorSitesSummaryReporter {
    pub const ID: &'static str = "error-sites-summary";
}

impl Reporter for ErrorSitesSummaryReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = error_site_rows(findings);
        let counts = kind_counts(&rows);
        let total = counts.total();

        let mut body = String::new();
        body.push_str("# Error sites summary (intermediate)\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** sites — `?` **{}**, map_err **{}**, return Err **{}**, \
             if let Err **{}**, match Err **{}**, ok_or **{}**.\n\n",
            counts.question_mark,
            counts.map_err,
            counts.return_err,
            counts.if_let_err,
            counts.match_err,
            counts.ok_or,
        ));
        body.push_str(
            "| Crate | Total | `?` | map_err | return Err | if let Err | match Err | ok_or |\n",
        );
        body.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
        let all_rows: Vec<&ErrorSiteRow> = rows.iter().collect();
        for crate_name in crate_names(&all_rows) {
            let crate_rows: Vec<ErrorSiteRow> = rows
                .iter()
                .filter(|row| row.crate_name == crate_name)
                .cloned()
                .collect();
            let crate_counts = kind_counts(&crate_rows);
            body.push_str(&format!(
                "| `{crate_name}` | {} | {} | {} | {} | {} | {} | {} |\n",
                crate_counts.total(),
                crate_counts.question_mark,
                crate_counts.map_err,
                crate_counts.return_err,
                crate_counts.if_let_err,
                crate_counts.match_err,
                crate_counts.ok_or,
            ));
        }
        body.push_str(&format!(
            "\n| **Total** | **{total}** | **{}** | **{}** | **{}** | **{}** | **{}** | **{}** |\n",
            counts.question_mark,
            counts.map_err,
            counts.return_err,
            counts.if_let_err,
            counts.match_err,
            counts.ok_or,
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "error-sites-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `error-sites-partitioned.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorSitesPartitionedCsvReporter;

impl ErrorSitesPartitionedCsvReporter {
    pub const ID: &'static str = "error-sites-partitioned-csv";
}

impl Reporter for ErrorSitesPartitionedCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from(
            "crate,site_kind,origin_class,origin_detail,rationale,context,file,line,source_snippet,site_snippet\n",
        );
        for row in error_site_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.site_kind),
                csv_field(&row.origin_class),
                csv_field(&row.origin_detail),
                csv_field(&row.rationale),
                csv_field(&row.context),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.source_snippet),
                csv_field(&row.site_snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "error-sites-partitioned.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `error-sites-partition-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ErrorSitesPartitionSummaryReporter;

impl ErrorSitesPartitionSummaryReporter {
    pub const ID: &'static str = "error-sites-partition-summary";
}

impl Reporter for ErrorSitesPartitionSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = error_site_rows(findings);
        let counts = origin_counts(&rows);
        let total = rows.len();
        let foreign_pool = counts.other + counts.edge;

        let mut body = String::new();
        body.push_str("# Error sites partition summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** sites — internal **{}**, other **{}**, edge **{}**, \
             foreign pool **{foreign_pool}** (other + edge).\n\n",
            counts.internal, counts.other, counts.edge,
        ));
        body.push_str("| Crate | Total | Internal | Other | Edge | Foreign pool |\n");
        body.push_str("| --- | ---: | ---: | ---: | ---: | ---: |\n");
        let all_rows: Vec<&ErrorSiteRow> = rows.iter().collect();
        for crate_name in crate_names(&all_rows) {
            let crate_rows: Vec<ErrorSiteRow> = rows
                .iter()
                .filter(|row| row.crate_name == crate_name)
                .cloned()
                .collect();
            let crate_counts = origin_counts(&crate_rows);
            let crate_foreign_pool = crate_counts.other + crate_counts.edge;
            body.push_str(&format!(
                "| `{crate_name}` | {} | {} | {} | {} | {crate_foreign_pool} |\n",
                crate_rows.len(),
                crate_counts.internal,
                crate_counts.other,
                crate_counts.edge,
            ));
        }
        body.push_str(&format!(
            "\n| **Total** | **{total}** | **{}** | **{}** | **{}** | **{foreign_pool}** |\n",
            counts.internal, counts.other, counts.edge,
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "error-sites-partition-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
