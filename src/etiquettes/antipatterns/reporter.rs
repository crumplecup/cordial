use std::collections::BTreeMap;

use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};

use crate::error::CordialResult;
use crate::hooks::Reporter;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};
use crate::session::SessionView;

use super::types::{AntipatternRuleId, build_workspace_antipatterns_summary};

#[derive(Debug, Default, Clone)]
pub(super) struct AntipatternRow {
    pub(super) crate_name: String,
    pub(super) rule_id: String,
    pub(super) context: String,
    pub(super) file: String,
    pub(super) line: String,
    pub(super) snippet: String,
    pub(super) disposition: String,
    pub(super) suppression_reason: String,
}

impl AntipatternRow {
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
            context: field("context"),
            file: field("file"),
            line: field("line"),
            snippet: field("snippet"),
            disposition: finding.disposition().to_string(),
            suppression_reason: field("suppression_reason"),
        }
    }
}

pub(super) fn antipattern_rows(findings: &[&dyn Finding]) -> Vec<AntipatternRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "antipatterns")
        .map(|finding| AntipatternRow::from_finding(*finding))
        .collect()
}

fn open_rows(rows: &[AntipatternRow]) -> impl Iterator<Item = &AntipatternRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

pub(super) fn escape_csv(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Writes `antipatterns.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AntipatternCsvReporter;

impl AntipatternCsvReporter {
    pub const ID: &'static str = "antipattern-csv";
}

impl Reporter for AntipatternCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let mut body = String::from("crate,rule_id,context,file,line,snippet\n");
        for row in open_rows(&antipattern_rows(findings)) {
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
            name: "antipatterns.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `antipatterns.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AntipatternChecklistReporter;

impl AntipatternChecklistReporter {
    pub const ID: &'static str = "antipattern-checklist";
}

impl Reporter for AntipatternChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = antipattern_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let suppressed: Vec<_> = rows
            .iter()
            .filter(|row| row.disposition == "suppressed")
            .collect();

        let mut body = String::new();
        body.push_str("# Antipatterns checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        if !suppressed.is_empty() {
            body.push_str(&format!(
                "**Documented exceptions:** {}\n\n",
                suppressed.len()
            ));
        }
        body.push_str(
            "Probe-based inventory of known antipatterns in crate `src/` trees. \
             Struct and enum variant fields should own their data instead of storing `&'static` \
             references in the ADT payload. Underscore-prefixed function parameters often indicate \
             trait impls that ignore required inputs. `Result<T, String>` (or `&str`) is an untyped \
             error carrier — wrap the payload in a newtype that implements `std::error::Error`. \
             A requires/ensures clause that matches no \
             registered `amenable_core::Ensures`/`Requires` contract fragment for its verifier is a \
             raw equation — mint a named contract type for the bound, `impl` `Ensures`/`Requires` on \
             it, and point the site at it. Workspace members should inherit crate and dependency \
             versions from the root `Cargo.toml` via `*.workspace = true` rather than repeating inline \
             `version = \"…\"` keys. Document accepted exceptions in \
             `{store}/exceptions/antipatterns/{{crate}}.json`.\n\n",
        );

        if !open.is_empty() || !suppressed.is_empty() {
            body.push_str(&format!("## `{}`\n\n", ir.crate_name()));
        }

        if !open.is_empty() {
            write_finding_sections(&mut body, &open)?;
        }
        if !suppressed.is_empty() {
            body.push_str("### Documented exceptions\n\n");
            for entry in suppressed {
                body.push_str(&format!(
                    "- [x] `{}` — `{}:{}` — `{}` — _{}_\n",
                    entry.context, entry.file, entry.line, entry.snippet, entry.suppression_reason
                ));
            }
            body.push('\n');
        }

        Ok(vec![Box::new(TextArtifact {
            name: "antipatterns.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

fn write_finding_sections(body: &mut String, findings: &[&AntipatternRow]) -> CordialResult<()> {
    let mut by_rule: BTreeMap<String, Vec<&AntipatternRow>> = BTreeMap::new();
    for finding in findings {
        by_rule
            .entry(finding.rule_id.clone())
            .or_default()
            .push(finding);
    }

    for (rule_id, entries) in by_rule {
        body.push_str(&format!("### {rule_id}\n\n"));
        if entries
            .first()
            .is_some_and(|f| f.rule_id == AntipatternRuleId::UnnamedContractBound001.as_str())
        {
            write_duplicate_clusters(body, &entries)?;
        }
        for entry in entries {
            body.push_str(&format!(
                "- [ ] `{}` — `{}:{}` — `{}`\n",
                entry.context, entry.file, entry.line, entry.snippet
            ));
        }
        body.push('\n');
    }
    Ok(())
}

fn write_duplicate_clusters(body: &mut String, entries: &[&AntipatternRow]) -> CordialResult<()> {
    let mut by_shape: BTreeMap<String, Vec<&AntipatternRow>> = BTreeMap::new();
    for entry in entries {
        by_shape
            .entry(clause_shape(&entry.snippet))
            .or_default()
            .push(entry);
    }

    let mut clusters: Vec<(&String, &Vec<&AntipatternRow>)> = by_shape
        .iter()
        .filter(|(_, members)| members.len() > 1)
        .collect();
    if clusters.is_empty() {
        return Ok(());
    }
    clusters.sort_by(|(shape_a, members_a), (shape_b, members_b)| {
        members_b
            .len()
            .cmp(&members_a.len())
            .then_with(|| shape_a.cmp(shape_b))
    });

    body.push_str(
        "**Possible duplicate clusters** (same clause shape, different variable or literal — \
         verify the sites actually share one claim, then name it once):\n",
    );
    for (shape, members) in clusters {
        body.push_str(&format!("- `{shape}` — {} sites:", members.len()));
        for (i, member) in members.iter().enumerate() {
            if i > 0 {
                body.push(',');
            }
            body.push_str(&format!(
                " `{}` (`{}:{}`)",
                member.context, member.file, member.line
            ));
        }
        body.push('\n');
    }
    body.push('\n');
    Ok(())
}

fn clause_shape(snippet: &str) -> String {
    snippet
        .parse::<TokenStream>()
        .map(|tokens| shape_tokens(tokens).to_string())
        .unwrap_or_else(|_| snippet.to_string())
}

fn shape_tokens(tokens: TokenStream) -> TokenStream {
    let items: Vec<TokenTree> = tokens.into_iter().collect();
    let mut out = Vec::with_capacity(items.len());
    for (i, tt) in items.iter().enumerate() {
        match tt {
            TokenTree::Ident(ident) => {
                let is_call = matches!(
                    items.get(i + 1),
                    Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Parenthesis
                );
                if is_call {
                    out.push(tt.clone());
                } else {
                    out.push(TokenTree::Ident(proc_macro2::Ident::new("X", ident.span())));
                }
            }
            TokenTree::Literal(literal) => {
                out.push(TokenTree::Ident(proc_macro2::Ident::new(
                    "X",
                    literal.span(),
                )));
            }
            TokenTree::Group(group) => {
                out.push(TokenTree::Group(Group::new(
                    group.delimiter(),
                    shape_tokens(group.stream()),
                )));
            }
            TokenTree::Punct(_) => out.push(tt.clone()),
        }
    }
    TokenStream::from_iter(out)
}

/// Writes `antipatterns-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AntipatternSummaryReporter;

impl AntipatternSummaryReporter {
    pub const ID: &'static str = "antipattern-summary";
}

impl Reporter for AntipatternSummaryReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let summary = build_workspace_antipatterns_summary(findings);
        let mut body = String::new();
        body.push_str("# Antipatterns summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{}** sites — box_dyn_error **{}**, string_error **{}**, unused_underscore_arg **{}**, \
             struct_static_ref **{}**, unnamed_contract_bound **{}**, version_in_member **{}**.\n\n",
            summary.total,
            summary.box_dyn_error,
            summary.string_error,
            summary.unused_underscore_arg,
            summary.struct_static_ref,
            summary.unnamed_contract_bound,
            summary.version_in_member
        ));
        body.push_str(
            "| Crate | Total | Box dyn error | String error | Unused underscore arg | Struct static ref | Unnamed contract bound | Version in member |\n",
        );
        body.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
        for row in &summary.crates {
            body.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
                row.crate_name,
                row.total,
                row.box_dyn_error,
                row.string_error,
                row.unused_underscore_arg,
                row.struct_static_ref,
                row.unnamed_contract_bound,
                row.version_in_member
            ));
        }
        body.push_str(&format!(
            "\n| **Total** | **{}** | **{}** | **{}** | **{}** | **{}** | **{}** | **{}** |\n",
            summary.total,
            summary.box_dyn_error,
            summary.string_error,
            summary.unused_underscore_arg,
            summary.struct_static_ref,
            summary.unnamed_contract_bound,
            summary.version_in_member
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "antipatterns-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
