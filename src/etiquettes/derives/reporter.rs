use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct DeriveRow {
    crate_name: String,
    rule_id: String,
    struct_name: String,
    method_name: String,
    qualified_name: String,
    recommendation: String,
    file: String,
    line: String,
    evidence: String,
    disposition: String,
}

impl DeriveRow {
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
            rule_id: field("rule_id"),
            struct_name: field("struct_name"),
            method_name: field("method_name"),
            qualified_name: field("qualified_name"),
            recommendation: field("recommendation"),
            file: field("file"),
            line: field("line"),
            evidence: field("evidence"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn derive_rows(findings: &[&dyn Finding]) -> Vec<DeriveRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "derives")
        .map(|finding| DeriveRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[DeriveRow]) -> impl Iterator<Item = &DeriveRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Writes `derives.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeriveCsvReporter;

impl DeriveCsvReporter {
    pub const ID: &'static str = "derive-csv";
}

impl Reporter for DeriveCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from(
            "crate,rule_id,struct_name,method_name,qualified_name,recommendation,file,line,evidence\n",
        );
        for row in derive_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                row.crate_name,
                row.rule_id,
                row.struct_name,
                row.method_name,
                row.qualified_name,
                escape_csv(&row.recommendation),
                row.file,
                row.line,
                escape_csv(&row.evidence),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "derives.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `derives.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeriveChecklistReporter;

impl DeriveChecklistReporter {
    pub const ID: &'static str = "derive-checklist";
}

impl Reporter for DeriveChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = derive_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# Derive patterns checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Replace hand-rolled builders with `derive_builder`, introduce a \
             builder when `new` has more arguments than `[derives].max_constructor_args`, \
             and replace trivial getters, setters, `new()`, and public struct fields \
             with `derive_getters`, `derive_setters`, or `derive_new`. `Some(arg)` \
             setters use `#[setters(strip_option)]`; `arg.into()` uses `#[setters(into)]`. \
             `as_ref()` / `as_str()` use `#[derive(derive_more::AsRef)]`. Error types \
             (and `#[track_caller]` constructors) skip `derive_new`.\n\n",
        );
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        let mut by_rule: BTreeMap<String, Vec<&DeriveRow>> = BTreeMap::new();
        for row in &open {
            by_rule.entry(row.rule_id.clone()).or_default().push(row);
        }

        for (rule_id, entries) in by_rule {
            body.push_str(&format!("### {rule_id}\n\n"));
            for entry in entries {
                let method = if entry.method_name.is_empty() {
                    String::new()
                } else {
                    format!(" `{}`", entry.method_name)
                };
                body.push_str(&format!(
                    "- [ ] `{}`{method} — `{}:{}` — {}\n",
                    entry.qualified_name, entry.file, entry.line, entry.recommendation
                ));
                body.push_str(&format!("  - _{}_\n", entry.evidence));
            }
            body.push('\n');
        }

        Ok(vec![Box::new(TextArtifact {
            name: "derives.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `derives-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeriveSummaryReporter;

impl DeriveSummaryReporter {
    pub const ID: &'static str = "derive-summary";
}

impl Reporter for DeriveSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = derive_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let total = open.len();
        let mut builder = 0usize;
        let mut use_builder = 0usize;
        let mut getter = 0usize;
        let mut setter = 0usize;
        let mut as_ref = 0usize;
        let mut as_str = 0usize;
        let mut new = 0usize;
        let mut pub_field = 0usize;

        for row in &open {
            match row.rule_id.as_str() {
                "DERIVE-BUILDER-001" => builder += 1,
                "DERIVE-USE-BUILDER-001" => use_builder += 1,
                "DERIVE-GETTER-001" => getter += 1,
                "DERIVE-SETTER-001" => setter += 1,
                "DERIVE-ASREF-001" => as_ref += 1,
                "DERIVE-ASSTR-001" => as_str += 1,
                "DERIVE-NEW-001" => new += 1,
                "DERIVE-PUB-FIELD-001" => pub_field += 1,
                _ => {}
            }
        }

        let mut body = String::new();
        body.push_str("# Derive patterns summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{total}** findings — builder **{builder}**, \
             use builder **{use_builder}**, getter **{getter}**, \
             setter **{setter}**, as_ref **{as_ref}**, as_str **{as_str}**, \
             new **{new}**, pub field **{pub_field}**.\n\n"
        ));
        body.push_str(
            "| Crate | Total | Builder | Use builder | Getter | Setter | AsRef | AsStr | New | Pub field |\n",
        );
        body.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
        body.push_str(&format!(
            "| `{}` | {total} | {builder} | {use_builder} | {getter} | {setter} | {as_ref} | {as_str} | {new} | {pub_field} |\n",
            ir.crate_name()
        ));
        body.push_str(&format!(
            "\n| **Total** | **{total}** | **{builder}** | **{use_builder}** | **{getter}** | **{setter}** | **{as_ref}** | **{as_str}** | **{new}** | **{pub_field}** |\n"
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "derives-summary.md".to_string(),
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
