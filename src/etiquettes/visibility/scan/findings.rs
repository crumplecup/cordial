//! Emit visibility records from a scanned module tree.

use super::eval::VisibilityEval;
use super::tree::{ModuleNode, external_name_count, public_path_mods};
use super::vis::VisKind;
use crate::etiquettes::visibility::types::{
    VisibilityRecord, VisibilityRuleId, VisibilityThresholds,
};

use tracing::instrument;
#[instrument(level = "debug", skip(root, thresholds, eval))]
pub(super) fn collect_findings(
    root: &ModuleNode,
    thresholds: VisibilityThresholds,
    eval: VisibilityEval,
) -> Vec<VisibilityRecord> {
    let external = external_name_count(root);
    let mut out = Vec::new();
    if external < thresholds.max_crate_names_for_flat {
        for pub_mod in public_path_mods(root) {
            out.push(VisibilityRecord {
                rule_id: VisibilityRuleId::CrateFlat001,
                module_path: pub_mod.path.clone(),
                file: pub_mod.file.clone(),
                line: pub_mod.line,
                name_count: external,
                parent_vis: "pub".to_string(),
                declared_vis: pub_mod.declared_vis.as_str().to_string(),
            });
        }
    }
    let thin_floor = eval.thin_floor(thresholds);
    collect_module_findings(root, thin_floor, &mut out);
    out.sort_by(|a, b| {
        a.rule_id
            .as_str()
            .cmp(b.rule_id.as_str())
            .then_with(|| a.module_path.cmp(&b.module_path))
    });
    out
}

#[instrument(level = "debug", skip(node, out))]
fn collect_module_findings(node: &ModuleNode, thin_floor: usize, out: &mut Vec<VisibilityRecord>) {
    if !node.is_crate_root {
        let mismatch = node.declared_vis.is_unrestricted_pub() && !node.parent_declared_pub;
        if mismatch {
            out.push(VisibilityRecord {
                rule_id: VisibilityRuleId::ModMismatch001,
                module_path: node.path.clone(),
                file: node.file.clone(),
                line: node.line,
                name_count: node.leaf_crate,
                parent_vis: if node.parent_declared_pub {
                    "pub".to_string()
                } else {
                    "non-pub".to_string()
                },
                declared_vis: node.declared_vis.as_str().to_string(),
            });
        }
        let is_path = node.declared_vis.is_unrestricted_pub()
            || node.declared_vis == VisKind::PubCrate
            || mismatch;
        if is_path {
            let count = if node.declared_vis.is_unrestricted_pub() && node.ancestors_all_pub {
                node.leaf_pub
            } else {
                node.leaf_crate
            };
            if count < thin_floor {
                out.push(VisibilityRecord {
                    rule_id: VisibilityRuleId::ModThin001,
                    module_path: node.path.clone(),
                    file: node.file.clone(),
                    line: node.line,
                    name_count: count,
                    parent_vis: if node.parent_declared_pub {
                        "pub".to_string()
                    } else {
                        "non-pub".to_string()
                    },
                    declared_vis: node.declared_vis.as_str().to_string(),
                });
            }
        }
    }
    for child in &node.children {
        collect_module_findings(child, thin_floor, out);
    }
}
