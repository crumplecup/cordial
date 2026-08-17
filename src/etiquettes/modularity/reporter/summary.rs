use crate::error::CordialResult;
use crate::hooks::Reporter;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding, TextArtifact};
use crate::session::SessionView;

use super::super::hierarchy::{
    ModuleHierarchyNode, build_module_hierarchy, fat_leaves, library_branches, lopsided_siblings,
    order_bands, top_heavy_parents,
};
use super::super::types::ModuleSizeStats;
use super::rows::{
    ModularityRow, SUMMARY_MODULE_ROWS, SUMMARY_RANK_ROWS, count_kind, crate_names,
    file_module_inputs, is_inventory_row, max_lines, modularity_rows, open_rows,
    sort_by_lines_desc,
};

/// Writes `modularity-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModularitySummaryReporter;

impl ModularitySummaryReporter {
    pub const ID: &'static str = "modularity-summary";
}

impl Reporter for ModularitySummaryReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let rows = modularity_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let thresholds = crate::config::load_session_config(session).modularity;
        let inventory: Vec<_> = open
            .iter()
            .copied()
            .filter(|row| is_inventory_row(row, &thresholds))
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
        let mut modules: Vec<_> = open
            .iter()
            .copied()
            .filter(|row| row.kind == "MODULARITY-MODULE-SIZE")
            .collect();
        sort_by_lines_desc(&mut modules);
        let sigma = thresholds.module_size_sigma;
        let min_lines = thresholds.min_module_lines;
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
             top-heavy **{top_heavy}**, lopsided **{lopsided}**.\n\n"
        ));
        body.push_str(
            "| Crate | Inventory | Checklist | Large files | Large functions | Types per file | Module outliers | Top-heavy | Lopsided | Largest file | Largest fn |\n",
        );
        body.push_str(
            "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
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
                "| `{crate_name}` | {} | {} | {} | {} | {} | {} | {} | {} | {largest_file} | {largest_fn} |\n",
                crate_open.len(),
                crate_checklist.len(),
                count_kind(&crate_checklist, "MODULARITY-FILE"),
                count_kind(&crate_checklist, "MODULARITY-FUNCTION"),
                count_kind(&crate_checklist, "MODULARITY-TYPES-PER-FILE"),
                count_kind(&crate_checklist, "MODULARITY-MODULE-SIZE"),
                count_kind(&crate_checklist, "MODULARITY-TOP-HEAVY"),
                count_kind(&crate_checklist, "MODULARITY-LOPSIDED"),
            ));
        }
        body.push_str(&format!(
            "| **Total** | **{inventory_total}** | **{checklist_total}** | **{large_files}** | **{large_functions}** | **{crowded_files}** | **{module_outliers}** | **{top_heavy}** | **{lopsided}** | — | — |\n"
        ));

        body.push_str("\n## Module sizes\n\n");
        if modules.is_empty() {
            body.push_str("_No modules inventoried._\n");
        } else {
            body.push_str(&format!(
                "**{}** modules in the σ sample (min {} lines), mean **{:.1}** lines, \
                 σ **{:.1}**. Outliers first, then the next-largest; |z| > {sigma} is a \
                 checklist lint. Full inventory is `modularity.csv`.\n\n",
                stats.n, min_lines, stats.mean, stats.stddev
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

fn append_hierarchy_sections(
    body: &mut String,
    modules: &[&ModularityRow],
    thresholds: crate::config::ModularityThresholds,
) {
    let names = crate_names(modules);
    if names.is_empty() {
        return;
    }

    body.push_str("\n## Stream order\n\n");
    body.push_str(
        "Horton–Strahler order on the file-module tree (leaves = 1; a parent \
         rises when two or more children share the max order). Subtree size \
         should grow with order; own size staying large at high order is top-heavy. \
         This chart is diagnostic — actions are in the checklist Rebalance section.\n\n",
    );
    for crate_name in &names {
        let tree = crate_tree(modules, crate_name);
        if tree.len() < 2 {
            continue;
        }
        if names.len() > 1 {
            body.push_str(&format!("### `{crate_name}`\n\n"));
        }
        body.push_str("| Order | Modules | Mean own | Mean subtree |\n");
        body.push_str("| ---: | ---: | ---: | ---: |\n");
        for band in order_bands(&tree) {
            body.push_str(&format!(
                "| {} | {} | {:.1} | {:.1} |\n",
                band.order, band.count, band.mean_own, band.mean_subtree
            ));
        }
        body.push('\n');
    }

    append_ranked_branches(body, modules, &names, thresholds);
    append_fat_leaves(body, modules, &names);
    append_top_heavy_parents(body, modules, &names, thresholds);
    append_lopsided(body, modules, &names, thresholds);
}

fn crate_tree(modules: &[&ModularityRow], crate_name: &str) -> Vec<ModuleHierarchyNode> {
    let crate_rows: Vec<_> = modules
        .iter()
        .copied()
        .filter(|row| row.crate_name == crate_name)
        .collect();
    build_module_hierarchy(&file_module_inputs(&crate_rows))
}

fn append_ranked_branches(
    body: &mut String,
    modules: &[&ModularityRow],
    names: &[String],
    thresholds: crate::config::ModularityThresholds,
) {
    body.push_str("## Library branches\n\n");
    body.push_str(
        "Crate-root packages that already have nested modules, ranked by \
         top-heaviness (`own / subtree`). A Hit is a peel-the-parent checklist \
         item. Undecomposed crate-root files are under Fat leaves.\n\n",
    );
    let mut any = false;
    for crate_name in names {
        let tree = crate_tree(modules, crate_name);
        let branches = library_branches(&tree);
        if branches.is_empty() {
            continue;
        }
        any = true;
        if names.len() > 1 {
            body.push_str(&format!("### `{crate_name}`\n\n"));
        }
        body.push_str("| Branch | Order | Own | Subtree | Top-heavy | Hit |\n");
        body.push_str("| --- | ---: | ---: | ---: | ---: | --- |\n");
        for node in branches {
            let hit = if thresholds.is_top_heavy_hit(node.own_lines, node.subtree_lines) {
                "yes"
            } else {
                ""
            };
            body.push_str(&format!(
                "| `{}` | {} | {} | {} | {:.2} | {hit} |\n",
                node.path,
                node.order,
                node.own_lines,
                node.subtree_lines,
                node.top_heavy()
            ));
        }
        body.push('\n');
    }
    if !any {
        body.push_str(
            "_No crate-level packages with children._ Ranked in `modularity-branches.csv`.\n\n",
        );
    }
}

fn append_fat_leaves(body: &mut String, modules: &[&ModularityRow], names: &[String]) {
    body.push_str("## Fat leaves\n\n");
    body.push_str(
        "Largest file modules with no children — they never grew a subtree. \
         Distinct from a parent that kept most of the mass. Too-long fat leaves \
         get extract-helpers first, then grow-a-subtree if those helpers belong \
         together as a named layer; this table is the ranking.\n\n",
    );
    let mut rows = Vec::new();
    for crate_name in names {
        let tree = crate_tree(modules, crate_name);
        for node in fat_leaves(&tree).into_iter().take(SUMMARY_RANK_ROWS) {
            rows.push((crate_name.clone(), node.clone()));
        }
    }
    rows.sort_by(|left, right| {
        right
            .1
            .own_lines
            .cmp(&left.1.own_lines)
            .then_with(|| left.1.path.cmp(&right.1.path))
    });
    rows.truncate(SUMMARY_RANK_ROWS);
    if rows.is_empty() {
        body.push_str("_No file-module leaves._\n\n");
        return;
    }
    body.push_str("| Crate | Module | File | Lines |\n");
    body.push_str("| --- | --- | --- | ---: |\n");
    for (crate_name, node) in rows {
        body.push_str(&format!(
            "| `{crate_name}` | `{}` | `{}` | {} |\n",
            node.path, node.file, node.own_lines
        ));
    }
    body.push('\n');
}

fn append_top_heavy_parents(
    body: &mut String,
    modules: &[&ModularityRow],
    names: &[String],
    thresholds: crate::config::ModularityThresholds,
) {
    body.push_str("## Top-heavy parents\n\n");
    body.push_str(&format!(
        "Nested modules that kept at least {}% of their subtree in their own file \
         (and at least {} own lines). Action: peel remaining mass into children. \
         Hits are checklist items.\n\n",
        thresholds.top_heavy_min_percent, thresholds.hierarchy_min_lines,
    ));
    let mut rows = Vec::new();
    for crate_name in names {
        let tree = crate_tree(modules, crate_name);
        for node in top_heavy_parents(&tree) {
            if thresholds.is_top_heavy_hit(node.own_lines, node.subtree_lines)
                || node.top_heavy() >= f64::from(thresholds.top_heavy_min_percent) / 100.0
            {
                rows.push((crate_name.clone(), node.clone()));
            }
        }
    }
    rows.sort_by(|left, right| {
        right
            .1
            .top_heavy()
            .total_cmp(&left.1.top_heavy())
            .then_with(|| right.1.own_lines.cmp(&left.1.own_lines))
            .then_with(|| left.1.path.cmp(&right.1.path))
    });
    rows.truncate(SUMMARY_RANK_ROWS);
    if rows.is_empty() {
        body.push_str("_No parent kept half or more of its subtree._\n\n");
        return;
    }
    body.push_str("| Crate | Module | Own | Subtree | Top-heavy | Children | Hit |\n");
    body.push_str("| --- | --- | ---: | ---: | ---: | ---: | --- |\n");
    for (crate_name, node) in rows {
        let hit = if thresholds.is_top_heavy_hit(node.own_lines, node.subtree_lines) {
            "yes"
        } else {
            ""
        };
        body.push_str(&format!(
            "| `{crate_name}` | `{}` | {} | {} | {:.2} | {} | {hit} |\n",
            node.path,
            node.own_lines,
            node.subtree_lines,
            node.top_heavy(),
            node.child_count
        ));
    }
    body.push('\n');
}

fn append_lopsided(
    body: &mut String,
    modules: &[&ModularityRow],
    names: &[String],
    thresholds: crate::config::ModularityThresholds,
) {
    body.push_str("## Lopsided siblings\n\n");
    body.push_str(&format!(
        "One child holding at least {}% of the siblings' combined subtree \
         after dropping siblings below {} lines. Action: split the dominant \
         child. Hits are checklist items.\n\n",
        thresholds.lopsided_min_percent, thresholds.hierarchy_min_lines,
    ));
    let mut rows = Vec::new();
    for crate_name in names {
        let tree = crate_tree(modules, crate_name);
        for imbalance in lopsided_siblings(&tree, thresholds.hierarchy_min_lines) {
            if thresholds.is_lopsided_hit(imbalance.largest_subtree, imbalance.sibling_total)
                || imbalance.share >= f64::from(thresholds.lopsided_min_percent) / 100.0
            {
                rows.push((crate_name.clone(), imbalance));
            }
        }
    }
    rows.sort_by(|left, right| {
        right
            .1
            .share
            .total_cmp(&left.1.share)
            .then_with(|| right.1.largest_subtree.cmp(&left.1.largest_subtree))
            .then_with(|| left.1.parent.cmp(&right.1.parent))
    });
    rows.truncate(SUMMARY_RANK_ROWS);
    if rows.is_empty() {
        body.push_str(&format!(
            "_No sibling group where one child holds {}%+ of the combined subtree._\n\n",
            thresholds.lopsided_min_percent
        ));
        return;
    }
    body.push_str("| Crate | Parent | Dominant child | Child subtree | Share | Siblings | Hit |\n");
    body.push_str("| --- | --- | --- | ---: | ---: | ---: | --- |\n");
    for (crate_name, imbalance) in rows {
        let hit = if thresholds.is_lopsided_hit(imbalance.largest_subtree, imbalance.sibling_total)
        {
            "yes"
        } else {
            ""
        };
        body.push_str(&format!(
            "| `{crate_name}` | `{}` | `{}` | {} | {:.2} | {} | {hit} |\n",
            imbalance.parent,
            imbalance.largest,
            imbalance.largest_subtree,
            imbalance.share,
            imbalance.sibling_count
        ));
    }
    body.push('\n');
}
