use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::Reporter;
use crate::ir::IrView;
use crate::objects::{Artifact, Disposition, Finding, MapFindingSink, TextArtifact};
use crate::session::SessionView;

/// Executive summary across all etiquettes in one run.
#[derive(Debug, Default, Clone, Copy)]
pub struct RollupReporter;

impl RollupReporter {
    pub const ID: &'static str = "rollup";
}

impl Reporter for RollupReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let mut by_category: BTreeMap<String, CategoryCounts> = BTreeMap::new();
        let mut by_rule: BTreeMap<String, usize> = BTreeMap::new();

        for finding in findings {
            let category = finding.rule().category().to_string();
            let counts = by_category.entry(category).or_default();
            match finding.disposition() {
                Disposition::Open => counts.open += 1,
                Disposition::Exemplar => counts.exemplar += 1,
                Disposition::Suppressed => counts.suppressed += 1,
            }
            if finding.disposition() == Disposition::Open {
                *by_rule.entry(finding.rule().id().to_string()).or_default() += 1;
            }
        }

        let open_total = by_category
            .values()
            .map(|counts| counts.open)
            .sum::<usize>();
        let suppressed_total = by_category
            .values()
            .map(|counts| counts.suppressed)
            .sum::<usize>();

        let mut crate_names = BTreeMap::new();
        for finding in findings {
            let mut sink = MapFindingSink::default();
            finding.emit(&mut sink);
            if let Some(crate_name) = field(&sink, "crate") {
                crate_names.insert(crate_name, ());
            }
        }
        let crates_line = if crate_names.is_empty() {
            ir.crate_name().to_string()
        } else {
            crate_names.keys().cloned().collect::<Vec<_>>().join("`, `")
        };

        let mut body = String::new();
        body.push_str("# Cordial rollup summary\n\n");
        body.push_str(&format!(
            "**Project:** `{}`\n\n**Crates:** `{crates_line}`\n\n**Store:** `{}`\n\n",
            session.project_root().display(),
            session.store_root().display(),
        ));
        body.push_str(&format!(
            "**Open findings:** {open_total} · **Suppressed:** {suppressed_total}\n\n",
        ));

        if by_category.is_empty() {
            body.push_str("_No findings emitted._\n");
        } else {
            body.push_str("## By etiquette\n\n");
            body.push_str("| Etiquette | Open | Exemplar | Suppressed | Total |\n");
            body.push_str("| --- | ---: | ---: | ---: | ---: |\n");
            for (category, counts) in &by_category {
                body.push_str(&format!(
                    "| `{category}` | {} | {} | {} | {} |\n",
                    counts.open,
                    counts.exemplar,
                    counts.suppressed,
                    counts.total(),
                ));
            }
            body.push('\n');
        }

        if !by_rule.is_empty() {
            body.push_str("## Open findings by rule\n\n");
            body.push_str("| Rule | Count |\n");
            body.push_str("| --- | ---: |\n");
            for (rule, count) in by_rule {
                body.push_str(&format!("| `{rule}` | {count} |\n"));
            }
            body.push('\n');
        }

        let open_findings: Vec<_> = findings
            .iter()
            .copied()
            .filter(|finding| finding.disposition() == Disposition::Open)
            .collect();
        if !open_findings.is_empty() {
            body.push_str("## Open items\n\n");
            for finding in open_findings {
                let mut sink = MapFindingSink::default();
                finding.emit(&mut sink);
                let context = field(&sink, "context").unwrap_or_else(|| "?".to_string());
                let file = field(&sink, "file");
                let line = field(&sink, "line");
                let location = match (file.as_deref(), line.as_deref()) {
                    (Some(file), Some(line)) => format!(" (`{file}:{line}`)"),
                    _ => String::new(),
                };
                body.push_str(&format!(
                    "- **{}** — `{context}`{location}\n",
                    finding.rule().id(),
                ));
            }
            body.push('\n');
        }

        Ok(vec![Box::new(TextArtifact {
            name: "rollup-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct CategoryCounts {
    open: usize,
    exemplar: usize,
    suppressed: usize,
}

impl CategoryCounts {
    fn total(self) -> usize {
        self.open + self.exemplar + self.suppressed
    }
}

fn field(sink: &MapFindingSink, name: &str) -> Option<String> {
    sink.fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}
