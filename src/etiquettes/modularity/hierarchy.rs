//! Horton–Strahler order on the crate module tree, and top-heaviness of branches.
//!
//! Leaves are order 1. A parent is `k + 1` when at least two children have
//! order `k`, otherwise it keeps the max child order — the same rule used
//! for stream networks. Top-heaviness is the fraction of a node's subtree
//! lines that live in the node itself (`own / subtree`).

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// One file-backed module, keyed by its `foo::bar` path (`<crate>` for root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSizeInput {
    pub path: String,
    pub file: String,
    pub lines: u32,
}

/// A node in the module tree after order and subtree sizes are filled in.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleHierarchyNode {
    pub path: String,
    pub file: String,
    pub own_lines: u32,
    pub subtree_lines: u32,
    pub order: u32,
    pub depth: u32,
    pub child_count: usize,
    /// Direct child module paths (file modules only).
    pub children: Vec<String>,
}

impl ModuleHierarchyNode {
    /// Fraction of subtree lines that live in this module's own file.
    pub fn top_heavy(&self) -> f64 {
        if self.subtree_lines == 0 {
            0.0
        } else {
            f64::from(self.own_lines) / f64::from(self.subtree_lines)
        }
    }

    pub fn is_branch(&self) -> bool {
        self.child_count > 0
    }
}

pub const CRATE_ROOT_PATH: &str = "<crate>";

fn parent_path(path: &str) -> Option<String> {
    if path == CRATE_ROOT_PATH {
        None
    } else if let Some((parent, _)) = path.rsplit_once("::") {
        Some(parent.to_string())
    } else {
        Some(CRATE_ROOT_PATH.to_string())
    }
}

fn nearest_parent(path: &str, existing: &BTreeSet<String>) -> Option<String> {
    let mut current = parent_path(path)?;
    loop {
        if existing.contains(&current) {
            return Some(current);
        }
        match parent_path(&current) {
            Some(next) => current = next,
            None => return None,
        }
    }
}

/// Build the module tree and assign Strahler order, depth, and subtree size.
pub fn build_module_hierarchy(modules: &[ModuleSizeInput]) -> Vec<ModuleHierarchyNode> {
    if modules.is_empty() {
        return Vec::new();
    }

    let mut by_path: BTreeMap<String, ModuleSizeInput> = BTreeMap::new();
    for module in modules {
        by_path
            .entry(module.path.clone())
            .and_modify(|existing| {
                if module.lines > existing.lines {
                    *existing = module.clone();
                }
            })
            .or_insert_with(|| module.clone());
    }
    let existing: BTreeSet<String> = by_path.keys().cloned().collect();

    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in &existing {
        if let Some(parent) = nearest_parent(path, &existing) {
            children.entry(parent).or_default().push(path.clone());
        }
    }
    for kids in children.values_mut() {
        kids.sort();
    }

    let mut order: HashMap<String, u32> = HashMap::new();
    let mut subtree: HashMap<String, u32> = HashMap::new();
    let mut depth: HashMap<String, u32> = HashMap::new();

    fn strahler(
        path: &str,
        children: &BTreeMap<String, Vec<String>>,
        order: &mut HashMap<String, u32>,
    ) -> u32 {
        if let Some(known) = order.get(path) {
            return *known;
        }
        let value = match children.get(path) {
            Some(kids) if !kids.is_empty() => {
                let child_orders: Vec<u32> = kids
                    .iter()
                    .map(|child| strahler(child, children, order))
                    .collect();
                let max = child_orders.iter().copied().max().unwrap_or(1);
                let ties = child_orders.iter().filter(|order| **order == max).count();
                if ties >= 2 { max + 1 } else { max }
            }
            _ => 1,
        };
        order.insert(path.to_string(), value);
        value
    }

    fn subtree_size(
        path: &str,
        by_path: &BTreeMap<String, ModuleSizeInput>,
        children: &BTreeMap<String, Vec<String>>,
        subtree: &mut HashMap<String, u32>,
    ) -> u32 {
        if let Some(known) = subtree.get(path) {
            return *known;
        }
        let own = by_path.get(path).map(|module| module.lines).unwrap_or(0);
        let child_total: u32 = children
            .get(path)
            .map(|kids| {
                kids.iter()
                    .map(|child| subtree_size(child, by_path, children, subtree))
                    .sum()
            })
            .unwrap_or(0);
        let value = own.saturating_add(child_total);
        subtree.insert(path.to_string(), value);
        value
    }

    fn assign_depth(
        path: &str,
        value: u32,
        children: &BTreeMap<String, Vec<String>>,
        depth: &mut HashMap<String, u32>,
    ) {
        depth.insert(path.to_string(), value);
        if let Some(kids) = children.get(path) {
            for child in kids {
                assign_depth(child, value.saturating_add(1), children, depth);
            }
        }
    }

    let roots: Vec<String> = existing
        .iter()
        .filter(|path| nearest_parent(path, &existing).is_none())
        .cloned()
        .collect();
    for root in &roots {
        assign_depth(root, 0, &children, &mut depth);
        strahler(root, &children, &mut order);
        subtree_size(root, &by_path, &children, &mut subtree);
    }

    by_path
        .into_iter()
        .map(|(path, input)| {
            let kids = children.get(&path).cloned().unwrap_or_default();
            let child_count = kids.len();
            ModuleHierarchyNode {
                own_lines: input.lines,
                subtree_lines: subtree.get(&path).copied().unwrap_or(input.lines),
                order: order.get(&path).copied().unwrap_or(1),
                depth: depth.get(&path).copied().unwrap_or(0),
                child_count,
                children: kids,
                path,
                file: input.file,
            }
        })
        .collect()
}

/// Direct children of the crate root that have nested modules, ranked most
/// top-heavy first. Undecomposed crate-root files are [`fat_leaves`], not this.
pub fn library_branches(nodes: &[ModuleHierarchyNode]) -> Vec<&ModuleHierarchyNode> {
    let mut branches: Vec<&ModuleHierarchyNode> = nodes
        .iter()
        .filter(|node| node.depth == 1 && node.is_branch())
        .collect();
    branches.sort_by(|left, right| {
        right
            .top_heavy()
            .total_cmp(&left.top_heavy())
            .then_with(|| right.own_lines.cmp(&left.own_lines))
            .then_with(|| left.path.cmp(&right.path))
    });
    branches
}

/// File modules with no children, ranked largest first. These never grew a
/// subtree — a different failure mode from a parent that kept the mass.
pub fn fat_leaves(nodes: &[ModuleHierarchyNode]) -> Vec<&ModuleHierarchyNode> {
    let mut leaves: Vec<&ModuleHierarchyNode> = nodes
        .iter()
        .filter(|node| !node.is_branch() && node.path != CRATE_ROOT_PATH)
        .collect();
    leaves.sort_by(|left, right| {
        right
            .own_lines
            .cmp(&left.own_lines)
            .then_with(|| left.path.cmp(&right.path))
    });
    leaves
}

/// Parents that retained subtree mass, at any depth except the crate root.
pub fn top_heavy_parents(nodes: &[ModuleHierarchyNode]) -> Vec<&ModuleHierarchyNode> {
    let mut parents: Vec<&ModuleHierarchyNode> = nodes
        .iter()
        .filter(|node| node.is_branch() && node.path != CRATE_ROOT_PATH)
        .collect();
    parents.sort_by(|left, right| {
        right
            .top_heavy()
            .total_cmp(&left.top_heavy())
            .then_with(|| right.own_lines.cmp(&left.own_lines))
            .then_with(|| left.path.cmp(&right.path))
    });
    parents
}

/// One child dominating its siblings' combined subtree.
#[derive(Debug, Clone, PartialEq)]
pub struct SiblingImbalance {
    pub parent: String,
    pub largest: String,
    pub largest_subtree: u32,
    pub sibling_total: u32,
    pub share: f64,
    pub sibling_count: usize,
    pub siblings: Vec<(String, u32)>,
}

/// Parents with two or more children, ranked by the largest child's share of
/// sibling subtree lines. Children below `min_child_lines` are omitted from
/// the group (a stub next to a real module is not lopsided).
pub fn lopsided_siblings(
    nodes: &[ModuleHierarchyNode],
    min_child_lines: u32,
) -> Vec<SiblingImbalance> {
    let by_path: HashMap<&str, &ModuleHierarchyNode> = nodes
        .iter()
        .map(|node| (node.path.as_str(), node))
        .collect();
    let mut ranked = Vec::new();
    for parent in nodes.iter().filter(|node| node.children.len() >= 2) {
        let mut kids: Vec<&ModuleHierarchyNode> = parent
            .children
            .iter()
            .filter_map(|path| by_path.get(path.as_str()).copied())
            .filter(|kid| kid.subtree_lines >= min_child_lines)
            .collect();
        if kids.len() < 2 {
            continue;
        }
        kids.sort_by(|left, right| {
            right
                .subtree_lines
                .cmp(&left.subtree_lines)
                .then_with(|| left.path.cmp(&right.path))
        });
        let total: u32 = kids.iter().map(|kid| kid.subtree_lines).sum();
        if total == 0 {
            continue;
        }
        let largest = kids[0];
        ranked.push(SiblingImbalance {
            parent: parent.path.clone(),
            largest: largest.path.clone(),
            largest_subtree: largest.subtree_lines,
            sibling_total: total,
            share: f64::from(largest.subtree_lines) / f64::from(total),
            sibling_count: kids.len(),
            siblings: kids
                .iter()
                .map(|kid| (kid.path.clone(), kid.subtree_lines))
                .collect(),
        });
    }
    ranked.sort_by(|left, right| {
        right
            .share
            .total_cmp(&left.share)
            .then_with(|| right.largest_subtree.cmp(&left.largest_subtree))
            .then_with(|| left.parent.cmp(&right.parent))
    });
    ranked
}

pub fn format_mass_list(entries: &[(String, u32)]) -> String {
    entries
        .iter()
        .map(|(path, lines)| format!("{path} {lines}"))
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn child_mass_list(node: &ModuleHierarchyNode, nodes: &[ModuleHierarchyNode]) -> String {
    let by_path: HashMap<&str, &ModuleHierarchyNode> = nodes
        .iter()
        .map(|item| (item.path.as_str(), item))
        .collect();
    let entries: Vec<(String, u32)> = node
        .children
        .iter()
        .filter_map(|path| {
            by_path
                .get(path.as_str())
                .map(|child| (child.path.clone(), child.subtree_lines))
        })
        .collect();
    format_mass_list(&entries)
}

/// Horton-style rollup: count and mean sizes at each Strahler order.
pub fn order_bands(nodes: &[ModuleHierarchyNode]) -> Vec<OrderBand> {
    let mut bands: BTreeMap<u32, (usize, u64, u64)> = BTreeMap::new();
    for node in nodes {
        let entry = bands.entry(node.order).or_insert((0, 0, 0));
        entry.0 += 1;
        entry.1 += u64::from(node.own_lines);
        entry.2 += u64::from(node.subtree_lines);
    }
    bands
        .into_iter()
        .map(|(order, (count, own_sum, subtree_sum))| OrderBand {
            order,
            count,
            mean_own: own_sum as f64 / count as f64,
            mean_subtree: subtree_sum as f64 / count as f64,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderBand {
    pub order: u32,
    pub count: usize,
    pub mean_own: f64,
    pub mean_subtree: f64,
}
