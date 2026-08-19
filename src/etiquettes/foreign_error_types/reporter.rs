use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::Reporter;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};
use crate::session::SessionView;

use super::types::{ForeignErrorRecordKind, build_workspace_foreign_error_type_summary};

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct ForeignErrorTypeRow {
    crate_name: String,
    record_kind: String,
    foreign_error_type: String,
    inference_rule_id: String,
    confidence: String,
    chain_break: String,
    site_kind: String,
    context: String,
    file: String,
    line: String,
    source_snippet: String,
    site_snippet: String,
    origin_class: String,
    origin_detail: String,
    disposition: String,
}

impl ForeignErrorTypeRow {
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
            record_kind: field("record_kind"),
            foreign_error_type: field("foreign_error_type"),
            inference_rule_id: field("inference_rule_id"),
            confidence: field("confidence"),
            chain_break: field("chain_break"),
            site_kind: field("site_kind"),
            context: field("context"),
            file: field("file"),
            line: field("line"),
            source_snippet: field("source_snippet"),
            site_snippet: field("site_snippet"),
            origin_class: field("origin_class"),
            origin_detail: field("origin_detail"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn foreign_error_type_rows(findings: &[&dyn Finding]) -> Vec<ForeignErrorTypeRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "foreign_error_types")
        .map(|finding| ForeignErrorTypeRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn typed_rows(rows: &[ForeignErrorTypeRow]) -> impl Iterator<Item = &ForeignErrorTypeRow> {
    rows.iter()
        .filter(|row| row.record_kind == ForeignErrorRecordKind::Typed.as_attr())
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[ForeignErrorTypeRow]) -> impl Iterator<Item = &ForeignErrorTypeRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

#[instrument(level = "debug")]
fn escape_csv(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[instrument(level = "debug", skip(rows))]
fn typed_report_from_rows(rows: &[ForeignErrorTypeRow]) -> super::types::ForeignErrorTypeReport {
    let crate_name = rows
        .first()
        .map(|row| row.crate_name.clone())
        .unwrap_or_default();
    let findings = rows
        .iter()
        .map(|row| super::types::ForeignErrorTypeRecord {
            crate_name: row.crate_name.clone(),
            foreign_error_type: row.foreign_error_type.clone(),
            rule_id: row.inference_rule_id.clone(),
            confidence: if row.confidence.contains("MEDIUM") {
                crate::etiquettes::error_sites::ForeignTypeConfidence::Medium
            } else {
                crate::etiquettes::error_sites::ForeignTypeConfidence::High
            },
            chain_break: row.chain_break == "true",
            kind: parse_site_kind(&row.site_kind),
            context: row.context.clone(),
            file: std::path::PathBuf::from(&row.file),
            line: row.line.parse().unwrap_or(0),
            source_snippet: row.source_snippet.clone(),
            site_snippet: row.site_snippet.clone(),
        })
        .collect();
    super::types::ForeignErrorTypeReport {
        crate_name,
        findings,
    }
}

#[instrument(level = "debug")]
fn parse_site_kind(value: &str) -> crate::etiquettes::error_sites::ErrorSiteKind {
    use crate::etiquettes::error_sites::ErrorSiteKind;
    match value {
        s if s == ErrorSiteKind::QuestionMark.to_string() => ErrorSiteKind::QuestionMark,
        s if s == ErrorSiteKind::MapErr.to_string() => ErrorSiteKind::MapErr,
        s if s == ErrorSiteKind::ReturnErr.to_string() => ErrorSiteKind::ReturnErr,
        s if s == ErrorSiteKind::IfLetErr.to_string() => ErrorSiteKind::IfLetErr,
        s if s == ErrorSiteKind::MatchErr.to_string() => ErrorSiteKind::MatchErr,
        s if s == ErrorSiteKind::OkOr.to_string() => ErrorSiteKind::OkOr,
        _ => ErrorSiteKind::QuestionMark,
    }
}

/// Writes `foreign-error-types.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ForeignErrorTypesCsvReporter;

impl ForeignErrorTypesCsvReporter {
    pub const ID: &'static str = "foreign-error-types-csv";
}

impl Reporter for ForeignErrorTypesCsvReporter {
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
            "crate,foreign_error_type,rule_id,confidence,chain_break,site_kind,context,file,line,source_snippet,site_snippet\n",
        );
        for row in typed_rows(&foreign_error_type_rows(findings)) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{}\n",
                row.crate_name,
                escape_csv(&row.foreign_error_type),
                row.inference_rule_id,
                row.confidence,
                row.chain_break,
                row.site_kind,
                row.context,
                row.file,
                row.line,
                escape_csv(&row.source_snippet),
                escape_csv(&row.site_snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "foreign-error-types.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `foreign-error-types.checklist.md` (chain breaks only).
#[derive(Debug, Default, Clone, Copy)]
pub struct ForeignErrorTypesChecklistReporter;

impl ForeignErrorTypesChecklistReporter {
    pub const ID: &'static str = "foreign-error-types-checklist";
}

impl Reporter for ForeignErrorTypesChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = foreign_error_type_rows(findings);
        let chain_breaks: Vec<_> = typed_rows(&rows)
            .filter(|row| row.chain_break == "true")
            .collect();

        let mut body = String::new();
        body.push_str("# Foreign error types checklist (chain breaks)\n\n");
        body.push_str(&format!("**Chain breaks:** {}\n\n", chain_breaks.len()));
        body.push_str(
            "Sites where `.map_err` **drops or stringifies** a std / third-party error. \
             Mapping into a typed constructor (`From`, `syn_parse`, forwarding `err` \
             plus caller context) is the preferred wrap and is omitted here. \
             Full inferred inventory is in `foreign-error-types.csv`. Resolution \
             strategies live in `foreign-error-attenuation.*`.\n\n",
        );

        let mut by_type: BTreeMap<&str, Vec<&ForeignErrorTypeRow>> = BTreeMap::new();
        for row in chain_breaks {
            by_type
                .entry(row.foreign_error_type.as_str())
                .or_default()
                .push(row);
        }

        for (foreign_type, type_rows) in by_type {
            body.push_str(&format!("## `{foreign_type}`\n\n"));
            for row in type_rows {
                body.push_str(&format!(
                    "- [ ] `{}` — `{}:{}` — `{}` — source `{source}` — rule `{rule}`\n",
                    row.context,
                    row.file,
                    row.line,
                    row.site_kind,
                    source = row.source_snippet,
                    rule = row.inference_rule_id,
                ));
            }
            body.push('\n');
        }

        Ok(vec![Box::new(TextArtifact {
            name: "foreign-error-types.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `foreign-error-types-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ForeignErrorTypesSummaryReporter;

impl ForeignErrorTypesSummaryReporter {
    pub const ID: &'static str = "foreign-error-types-summary";
}

impl Reporter for ForeignErrorTypesSummaryReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = foreign_error_type_rows(findings);
        let typed: Vec<_> = typed_rows(&rows).cloned().collect();
        let report = typed_report_from_rows(&typed);
        let summary = build_workspace_foreign_error_type_summary(&[report]);

        let mut body = String::new();
        body.push_str("# Foreign error types summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Inferred foreign exposure for `{}`: **{}** sites, **{}** chain breaks \
             (`.map_err` that drops or stringifies the foreign error).\n\n",
            ir.crate_name(),
            summary.inferred_sites,
            summary.chain_breaks,
        ));
        body.push_str("| Foreign error type | Chain breaks | Total inferred |\n");
        body.push_str("| --- | ---: | ---: |\n");
        for row in &summary.types {
            body.push_str(&format!(
                "| `{}` | {} | {} |\n",
                row.foreign_error_type, row.chain_breaks, row.total
            ));
        }
        body.push_str(&format!(
            "\n| **Total** | **{}** | **{}** |\n",
            summary.chain_breaks, summary.inferred_sites
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "foreign-error-types-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `foreign-errors.checklist.md` (other + edge partition candidates).
#[derive(Debug, Default, Clone, Copy)]
pub struct ForeignErrorsChecklistReporter;

impl ForeignErrorsChecklistReporter {
    pub const ID: &'static str = "foreign-errors-checklist";
}

impl Reporter for ForeignErrorsChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, findings, ir, _session))]
    fn render(
        &self,
        findings: &[&dyn Finding],
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = foreign_error_type_rows(findings);
        let candidates: Vec<_> = open_rows(&rows)
            .filter(|row| row.record_kind == ForeignErrorRecordKind::Candidate.as_attr())
            .collect();

        let mut body = String::new();
        body.push_str("# Foreign error candidates checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", candidates.len()));
        body.push_str(
            "Derived from Phase 2 partition of error sites. Rows classified **other** \
             (std / third-party before conversion) or **edge** (unresolved callee) form \
             the pool for foreign-error typing. Resolution strategies are out of scope.\n\n",
        );
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        let mut by_class: BTreeMap<String, Vec<&ForeignErrorTypeRow>> = BTreeMap::new();
        for row in candidates {
            by_class
                .entry(row.origin_class.clone())
                .or_default()
                .push(row);
        }

        for (class, class_rows) in by_class {
            body.push_str(&format!("### {class}\n\n"));
            let mut by_detail: BTreeMap<&str, Vec<&ForeignErrorTypeRow>> = BTreeMap::new();
            for row in class_rows {
                by_detail
                    .entry(row.origin_detail.as_str())
                    .or_default()
                    .push(row);
            }
            for (detail, detail_rows) in by_detail {
                body.push_str(&format!("#### `{detail}`\n\n"));
                for row in detail_rows {
                    body.push_str(&format!(
                        "- [ ] `{}` — `{}:{}` — `{}` — source `{source}`\n",
                        row.context,
                        row.file,
                        row.line,
                        row.site_kind,
                        source = row.source_snippet,
                    ));
                }
                body.push('\n');
            }
        }

        Ok(vec![Box::new(TextArtifact {
            name: "foreign-errors.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
