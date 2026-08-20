//! Stream-order, branch, and rebalance tables for `modularity-summary.md`.

use super::super::hierarchy::{
    ModuleHierarchyNode, build_module_hierarchy, fat_leaves, library_branches, lopsided_siblings,
    order_bands, top_heavy_parents, unary_nests,
};
use super::rows::{ModularityRow, SUMMARY_RANK_ROWS, crate_names, file_module_inputs};

use tracing::instrument;

#[instrument(level = "debug", skip(modules, thresholds))]
pub(super) fn append_hierarchy_sections(
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
    append_unary_nests(body, modules, &names, thresholds);
}

#[instrument(level = "debug", skip(modules))]
fn crate_tree(modules: &[&ModularityRow], crate_name: &str) -> Vec<ModuleHierarchyNode> {
    let crate_rows: Vec<_> = modules
        .iter()
        .copied()
        .filter(|row| row.crate_name == crate_name)
        .collect();
    build_module_hierarchy(&file_module_inputs(&crate_rows))
}

#[instrument(level = "debug", skip(modules, thresholds))]
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

#[instrument(level = "debug", skip(modules))]
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

#[instrument(level = "debug", skip(modules, thresholds))]
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
        thresholds.top_heavy_min_percent(), thresholds.hierarchy_min_lines(),
    ));
    let mut rows = Vec::new();
    for crate_name in names {
        let tree = crate_tree(modules, crate_name);
        for node in top_heavy_parents(&tree) {
            if thresholds.is_top_heavy_hit(node.own_lines, node.subtree_lines)
                || node.top_heavy() >= f64::from(thresholds.top_heavy_min_percent()) / 100.0
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

#[instrument(level = "debug", skip(modules, thresholds))]
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
        thresholds.lopsided_min_percent(), thresholds.hierarchy_min_lines(),
    ));
    let mut rows = Vec::new();
    for crate_name in names {
        let tree = crate_tree(modules, crate_name);
        for imbalance in lopsided_siblings(&tree, thresholds.hierarchy_min_lines()) {
            if thresholds.is_lopsided_hit(imbalance.largest_subtree, imbalance.sibling_total)
                || imbalance.share >= f64::from(thresholds.lopsided_min_percent()) / 100.0
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
            thresholds.lopsided_min_percent()
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

#[instrument(level = "debug", skip(modules, thresholds))]
fn append_unary_nests(
    body: &mut String,
    modules: &[&ModularityRow],
    names: &[String],
    thresholds: crate::config::ModularityThresholds,
) {
    body.push_str("## Unary nests\n\n");
    body.push_str(&format!(
        "A parent whose only child is itself a branch (subtree at least {} lines). \
         Action: collapse the extra directory and lift grandchildren into the parent. \
         A unary leaf is a peel, not this. Hits are checklist items.\n\n",
        thresholds.hierarchy_min_lines(),
    ));
    let mut rows = Vec::new();
    for crate_name in names {
        let tree = crate_tree(modules, crate_name);
        for nest in unary_nests(&tree, thresholds.hierarchy_min_lines()) {
            if thresholds.is_collapse_hit(nest.passthrough_subtree) {
                rows.push((crate_name.clone(), nest));
            }
        }
    }
    rows.sort_by(|left, right| {
        right
            .1
            .passthrough_subtree
            .cmp(&left.1.passthrough_subtree)
            .then_with(|| left.1.passthrough.cmp(&right.1.passthrough))
    });
    rows.truncate(SUMMARY_RANK_ROWS);
    if rows.is_empty() {
        body.push_str("_No unary child directory with a substantial subtree._\n\n");
        return;
    }
    body.push_str("| Crate | Parent | Passthrough | Own | Subtree | Grandchildren | Hit |\n");
    body.push_str("| --- | --- | --- | ---: | ---: | ---: | --- |\n");
    for (crate_name, nest) in rows {
        let hit = if thresholds.is_collapse_hit(nest.passthrough_subtree) {
            "yes"
        } else {
            ""
        };
        body.push_str(&format!(
            "| `{crate_name}` | `{}` | `{}` | {} | {} | {} | {hit} |\n",
            nest.parent,
            nest.passthrough,
            nest.passthrough_own,
            nest.passthrough_subtree,
            nest.grandchildren.len()
        ));
    }
    body.push('\n');
}
