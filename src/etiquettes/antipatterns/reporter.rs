use std::collections::BTreeMap;

use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use super::types::{AntipatternRuleId, build_workspace_antipatterns_summary};

use tracing::instrument;
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
            context: field("context"),
            file: field("file"),
            line: field("line"),
            snippet: field("snippet"),
            disposition: finding.disposition().to_string(),
            suppression_reason: field("suppression_reason"),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
pub(super) fn antipattern_rows(findings: &[&dyn Finding]) -> Vec<AntipatternRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "antipatterns")
        .map(|finding| AntipatternRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[AntipatternRow]) -> impl Iterator<Item = &AntipatternRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

/// Distinct crate names present in `rows`, sorted -- `view.ir.crate_name()`
/// is pinned to whichever crate the run's target discovery lists first, not
/// the crate a given row actually belongs to, so a workspace-spanning
/// artifact must derive its own crate breakdown from `row.crate_name`
/// instead (the same pattern `modularity::reporter::rows::crate_names` uses).
#[instrument(level = "debug", skip(rows))]
fn crate_names(rows: &[&AntipatternRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
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

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,rule_id,context,file,line,snippet\n");
        for row in open_rows(&antipattern_rows(findings)) {
            body.push_str(&format!(
                "{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.rule_id),
                csv_field(&row.context),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.snippet),
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

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

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
             references in the ADT payload. Copy `file` and `line` out of `Location` rather than \
             storing the reference; `&'static dyn Trait` is allowed when `Trait` is defined in this \
             crate (plugin tables and view structs). `&'static str` (and other static refs) are \
             allowed on types constructed only as `const`/`static` items; runtime ADTs should \
             `to_owned`. Underscore-prefixed function parameters often indicate \
             ignored inputs; impls of traits defined outside this crate are skipped because the \
             signature is not ours to shrink. `Result<T, String>` (or `&str`) is an untyped \
             error carrier — wrap the payload in a newtype that implements `std::error::Error`. \
             A requires/ensures clause that matches no \
             registered `amenable_core::Ensures`/`Requires` contract fragment for its verifier is a \
             raw equation — mint a named contract type for the bound, `impl` `Ensures`/`Requires` on \
             it, and point the site at it. Workspace members should inherit crate and dependency \
             versions from the root `Cargo.toml` via `*.workspace = true` rather than repeating inline \
             `version = \"…\"` keys. Document accepted exceptions in \
             `{store}/exceptions/antipatterns/{{crate}}.json`.\n\n",
        );

        let all: Vec<&AntipatternRow> = open
            .iter()
            .copied()
            .chain(suppressed.iter().copied())
            .collect();
        for crate_name in crate_names(&all) {
            let crate_open: Vec<_> = open
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            let crate_suppressed: Vec<_> = suppressed
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            body.push_str(&format!("## `{crate_name}`\n\n"));

            if !crate_open.is_empty() {
                write_finding_sections(&mut body, &crate_open)?;
            }
            if !crate_suppressed.is_empty() {
                body.push_str("### Documented exceptions\n\n");
                for entry in crate_suppressed {
                    body.push_str(&format!(
                        "- [x] `{}` — `{}:{}` — `{}` — _{}_\n",
                        entry.context,
                        entry.file,
                        entry.line,
                        entry.snippet,
                        entry.suppression_reason
                    ));
                }
                body.push('\n');
            }
        }

        Ok(vec![Box::new(TextArtifact {
            name: "antipatterns.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

#[instrument(level = "info", skip(findings), err(level = "warn"))]
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

#[instrument(level = "info", skip(entries), err(level = "warn"))]
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

#[instrument(level = "debug")]
fn clause_shape(snippet: &str) -> String {
    snippet
        .parse::<TokenStream>()
        .map(|tokens| shape_tokens(tokens).to_string())
        .unwrap_or_else(|_| snippet.to_string())
}

#[instrument(level = "debug", skip(tokens))]
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
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

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
