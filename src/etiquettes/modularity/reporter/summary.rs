use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, TextArtifact};

use super::super::types::ModuleSizeStats;
use super::rows::{
    ModularityRow, SUMMARY_MODULE_ROWS, SUMMARY_RANK_ROWS, count_kind, crate_names,
    is_inventory_row, max_lines, modularity_rows, open_rows, sort_by_lines_desc,
};
use super::structure::append_hierarchy_sections;

use tracing::instrument;
/// Writes `modularity-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModularitySummaryReporter;

impl ModularitySummaryReporter {
    pub const ID: &'static str = "modularity-summary";
}

impl Reporter for ModularitySummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let session = view.session;

        let rows = modularity_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let config = crate::config::load_session_config(session);
        let thresholds = config.modularity();
        let inventory: Vec<_> = open
            .iter()
            .copied()
            .filter(|row| is_inventory_row(row, thresholds))
            .collect();
        let inventory_total = inventory.len();
        let checklist: Vec<_> = open
            .iter()
            .copied()
            .filter(|row| row.is_checklist())
            .collect();
        let checklist_total = checklist.len();
        let large_files = count_kind(&checklist, "MODULARITY-FILE");
        let large_functions = count_kind(&checklist, "MODULARITY-FUNCTION");
        let crowded_files = count_kind(&checklist, "MODULARITY-TYPES-PER-FILE");
        let module_outliers = count_kind(&checklist, "MODULARITY-MODULE-SIZE");
        let top_heavy = count_kind(&checklist, "MODULARITY-TOP-HEAVY");
        let lopsided = count_kind(&checklist, "MODULARITY-LOPSIDED");
        let collapse = count_kind(&checklist, "MODULARITY-COLLAPSE");
        let mut modules: Vec<_> = open
            .iter()
            .copied()
            .filter(|row| row.kind == "MODULARITY-MODULE-SIZE")
            .collect();
        sort_by_lines_desc(&mut modules);
        let sigma = thresholds.module_size_sigma();
        let min_lines = thresholds.min_module_lines();
        let sample_lines: Vec<u32> = modules
            .iter()
            .filter_map(|row| row.lines.parse::<u32>().ok())
            .filter(|lines| *lines >= min_lines)
            .collect();
        let stats = ModuleSizeStats::from_lines(&sample_lines);

        let mut body = String::new();
        body.push_str("# Modularity summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{inventory_total}** inventory rows, **{checklist_total}** checklist items — large files **{large_files}**, \
             large functions **{large_functions}**, types-per-file **{crowded_files}**, module-size outliers **{module_outliers}**, \
             top-heavy **{top_heavy}**, lopsided **{lopsided}**, collapse **{collapse}**.\n\n"
        ));
        body.push_str(
            "| Crate | Inventory | Checklist | Large files | Large functions | Types per file | Module outliers | Top-heavy | Lopsided | Collapse | Largest file | Largest fn |\n",
        );
        body.push_str(
            "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
        );
        for crate_name in crate_names(&open) {
            let crate_open: Vec<_> = inventory
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            let crate_checklist: Vec<_> = crate_open
                .iter()
                .copied()
                .filter(|row| row.is_checklist())
                .collect();
            let largest_file = max_lines(&crate_open, "MODULARITY-FILE");
            let largest_fn = max_lines(&crate_open, "MODULARITY-FUNCTION");
            body.push_str(&format!(
                "| `{crate_name}` | {} | {} | {} | {} | {} | {} | {} | {} | {} | {largest_file} | {largest_fn} |\n",
                crate_open.len(),
                crate_checklist.len(),
                count_kind(&crate_checklist, "MODULARITY-FILE"),
                count_kind(&crate_checklist, "MODULARITY-FUNCTION"),
                count_kind(&crate_checklist, "MODULARITY-TYPES-PER-FILE"),
                count_kind(&crate_checklist, "MODULARITY-MODULE-SIZE"),
                count_kind(&crate_checklist, "MODULARITY-TOP-HEAVY"),
                count_kind(&crate_checklist, "MODULARITY-LOPSIDED"),
                count_kind(&crate_checklist, "MODULARITY-COLLAPSE"),
            ));
        }
        body.push_str(&format!(
            "| **Total** | **{inventory_total}** | **{checklist_total}** | **{large_files}** | **{large_functions}** | **{crowded_files}** | **{module_outliers}** | **{top_heavy}** | **{lopsided}** | **{collapse}** | — | — |\n"
        ));

        body.push_str("\n## Module sizes\n\n");
        if modules.is_empty() {
            body.push_str("_No modules inventoried._\n");
        } else {
            body.push_str(&format!(
                "**{}** modules in the σ sample (min {} lines), mean **{:.1}** lines, \
                 σ **{:.1}**. Outliers first, then the next-largest; |z| > {sigma} is a \
                 checklist lint on the upper tail only when lines >= {}{}. \
                 Full inventory is `modularity.csv`.\n\n",
                stats.n,
                min_lines,
                stats.mean,
                stats.stddev,
                thresholds.file_inventory_min_lines(),
                if thresholds.module_size_ignore_lower_tail() {
                    "; lower tail ignored"
                } else {
                    "; two-tailed"
                },
            ));
            append_truncated_module_table(&mut body, &modules);
        }
        append_longest_methods(&mut body, &inventory);
        if !modules.is_empty() {
            append_hierarchy_sections(&mut body, &modules, thresholds);
        }

        Ok(vec![Box::new(TextArtifact {
            name: "modularity-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

#[instrument(level = "debug", skip(modules))]
fn append_truncated_module_table(body: &mut String, modules: &[&ModularityRow]) {
    let mut shown: Vec<&ModularityRow> = modules
        .iter()
        .copied()
        .filter(|row| row.is_checklist())
        .collect();
    for row in modules {
        if shown.len() >= SUMMARY_MODULE_ROWS {
            break;
        }
        if shown.iter().any(|existing| {
            existing.crate_name == row.crate_name && existing.context == row.context
        }) {
            continue;
        }
        shown.push(*row);
    }

    body.push_str("| Crate | Module | File | Lines | z | Outlier |\n");
    body.push_str("| --- | --- | --- | ---: | ---: | --- |\n");
    for row in &shown {
        let outlier = if row.is_checklist() { "yes" } else { "" };
        let zscore = if row.zscore.is_empty() {
            "—"
        } else {
            row.zscore.as_str()
        };
        body.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} | {zscore} | {outlier} |\n",
            row.crate_name, row.context, row.file, row.lines
        ));
    }
    let hidden = modules.len().saturating_sub(shown.len());
    if hidden > 0 {
        body.push_str(&format!("\n_{hidden} more modules in `modularity.csv`._\n"));
    }
}

#[instrument(level = "debug", skip(open))]
fn append_longest_methods(body: &mut String, open: &[&ModularityRow]) {
    let mut methods: Vec<&ModularityRow> = open
        .iter()
        .copied()
        .filter(|row| row.kind == "MODULARITY-FUNCTION")
        .collect();
    if methods.is_empty() {
        return;
    }
    sort_by_lines_desc(&mut methods);
    let hidden = methods.len().saturating_sub(SUMMARY_RANK_ROWS);
    methods.truncate(SUMMARY_RANK_ROWS);
    body.push_str("\n## Longest method bodies\n\n");
    body.push_str(
        "Inventory, ranked by body lines — the split-the-body candidates, including \
         those still below the checklist cutoff.\n\n",
    );
    body.push_str("| Crate | Method | File | Lines | Checklist |\n");
    body.push_str("| --- | --- | --- | ---: | --- |\n");
    for row in methods {
        let flag = if row.is_checklist() { "yes" } else { "" };
        body.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} | {flag} |\n",
            row.crate_name, row.context, row.file, row.lines
        ));
    }
    if hidden > 0 {
        body.push_str(&format!(
            "\n_{hidden} more functions in `modularity.csv`._\n"
        ));
    }
}
