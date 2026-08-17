use crate::error::CordialResult;
use crate::hooks::Reporter;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};
use crate::session::SessionView;

#[derive(Debug, Default, Clone)]
struct VisibilityRow {
    crate_name: String,
    rule_id: String,
    module_path: String,
    file: String,
    line: String,
    name_count: String,
    parent_vis: String,
    declared_vis: String,
    disposition: String,
}

impl VisibilityRow {
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
            module_path: field("module_path"),
            file: field("file"),
            line: field("line"),
            name_count: field("name_count"),
            parent_vis: field("parent_vis"),
            declared_vis: field("declared_vis"),
            disposition: finding.disposition().to_string(),
        }
    }
}

fn visibility_rows(findings: &[&dyn Finding]) -> Vec<VisibilityRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "visibility")
        .map(|finding| VisibilityRow::from_finding(*finding))
        .collect()
}

fn open_rows(rows: &[VisibilityRow]) -> impl Iterator<Item = &VisibilityRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

fn crate_names(rows: &[&VisibilityRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

/// Writes `visibility.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct VisibilityCsvReporter;

impl VisibilityCsvReporter {
    pub const ID: &'static str = "visibility-csv";
}

impl Reporter for VisibilityCsvReporter {
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
            "crate,rule_id,module_path,file,line,name_count,parent_vis,declared_vis\n",
        );
        for row in open_rows(&visibility_rows(findings)) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                row.crate_name,
                row.rule_id,
                row.module_path,
                row.file,
                row.line,
                row.name_count,
                row.parent_vis,
                row.declared_vis,
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "visibility.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `visibility.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct VisibilityChecklistReporter;

impl VisibilityChecklistReporter {
    pub const ID: &'static str = "visibility-checklist";
}

impl Reporter for VisibilityChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = visibility_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let mut body = String::new();
        body.push_str("# Visibility checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "A visible module must earn its path: a small crate stays flat (`mod` + root \
             `pub use`); a `pub`/`pub(crate)` module needs enough leaf names; a child's \
             vis must not exceed its parent (`pub mod` under a private parent is a hole).\n\n",
        );
        if !open.is_empty() {
            for crate_name in crate_names(&open) {
                body.push_str(&format!("## `{crate_name}`\n\n"));
                for row in open.iter().filter(|row| row.crate_name == crate_name) {
                    body.push_str(&format!(
                        "- [ ] `{}` — `{}` — `{}` names, vis `{}` (parent `{}`)\n",
                        row.module_path,
                        row.rule_id,
                        row.name_count,
                        row.declared_vis,
                        row.parent_vis
                    ));
                }
                body.push('\n');
            }
        } else {
            body.push_str("_No visibility-path findings._\n\n");
        }
        Ok(vec![Box::new(TextArtifact {
            name: "visibility.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `visibility-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct VisibilitySummaryReporter;

impl VisibilitySummaryReporter {
    pub const ID: &'static str = "visibility-summary";
}

impl Reporter for VisibilitySummaryReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = visibility_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let flat = open
            .iter()
            .filter(|row| row.rule_id == "VIS-CRATE-FLAT-001")
            .count();
        let thin = open
            .iter()
            .filter(|row| row.rule_id == "VIS-MOD-THIN-001")
            .count();
        let mismatch = open
            .iter()
            .filter(|row| row.rule_id == "VIS-MOD-MISMATCH-001")
            .count();
        let mut body = String::new();
        body.push_str("# Visibility summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{}** open items — crate-flat **{flat}**, thin module **{thin}**, vis mismatch **{mismatch}**.\n\n",
            open.len()
        ));
        body.push_str("| Crate | Crate-flat | Thin | Mismatch |\n");
        body.push_str("| --- | ---: | ---: | ---: |\n");
        for crate_name in crate_names(&open) {
            let crate_rows: Vec<_> = open
                .iter()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            let crate_flat = crate_rows
                .iter()
                .filter(|row| row.rule_id == "VIS-CRATE-FLAT-001")
                .count();
            let crate_thin = crate_rows
                .iter()
                .filter(|row| row.rule_id == "VIS-MOD-THIN-001")
                .count();
            let crate_mismatch = crate_rows
                .iter()
                .filter(|row| row.rule_id == "VIS-MOD-MISMATCH-001")
                .count();
            body.push_str(&format!(
                "| `{crate_name}` | {crate_flat} | {crate_thin} | {crate_mismatch} |\n",
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "visibility-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
