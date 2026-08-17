use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::Reporter;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};
use crate::session::SessionView;

use super::types::{ErrorOriginClass, ErrorOriginClassCounts, ErrorSiteKind, ErrorSiteKindCounts};

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

fn error_site_rows(findings: &[&dyn Finding]) -> Vec<ErrorSiteRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "error_sites")
        .map(|finding| ErrorSiteRow::from_finding(*finding))
        .collect()
}

fn open_rows(rows: &[ErrorSiteRow]) -> impl Iterator<Item = &ErrorSiteRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

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

fn escape_csv(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
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

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let mut body =
            String::from("crate,site_kind,context,file,line,source_snippet,site_snippet\n");
        for row in error_site_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                row.crate_name,
                row.site_kind,
                row.context,
                row.file,
                row.line,
                escape_csv(&row.source_snippet),
                escape_csv(&row.site_snippet),
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

    fn render(
        &self,
        findings: &[&dyn Finding],
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
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
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        let mut by_kind: BTreeMap<String, Vec<&ErrorSiteRow>> = BTreeMap::new();
        for row in &open {
            by_kind.entry(row.site_kind.clone()).or_default().push(row);
        }

        for (kind, entries) in by_kind {
            body.push_str(&format!("### {kind}\n\n"));
            for entry in entries {
                body.push_str(&format!(
                    "- [ ] `{}` — `{}:{}` — source `{}` — `{}`\n",
                    entry.context, entry.file, entry.line, entry.source_snippet, entry.site_snippet,
                ));
            }
            body.push('\n');
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

    fn render(
        &self,
        findings: &[&dyn Finding],
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
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
        body.push_str(&format!(
            "| `{}` | {total} | {} | {} | {} | {} | {} | {} |\n",
            ir.crate_name(),
            counts.question_mark,
            counts.map_err,
            counts.return_err,
            counts.if_let_err,
            counts.match_err,
            counts.ok_or,
        ));
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

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let mut body = String::from(
            "crate,site_kind,origin_class,origin_detail,rationale,context,file,line,source_snippet,site_snippet\n",
        );
        for row in error_site_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}\n",
                row.crate_name,
                row.site_kind,
                row.origin_class,
                escape_csv(&row.origin_detail),
                escape_csv(&row.rationale),
                row.context,
                row.file,
                row.line,
                escape_csv(&row.source_snippet),
                escape_csv(&row.site_snippet),
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
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
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
        body.push_str(&format!(
            "| `{}` | {total} | {} | {} | {} | {foreign_pool} |\n",
            ir.crate_name(),
            counts.internal,
            counts.other,
            counts.edge,
        ));
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
