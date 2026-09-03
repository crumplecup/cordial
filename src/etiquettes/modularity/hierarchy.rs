//! Horton–Strahler order on the crate module tree, and top-heaviness of branches.
//!
//! Leaves are order 1. A parent is `k + 1` when at least two children have
//! order `k`, otherwise it keeps the max child order — the same rule used
//! for stream networks. Top-heaviness is the fraction of a node's subtree
//! lines that live in the node itself (`own / subtree`). Unary nests are a
//! parent whose only child is itself a branch.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use tracing::instrument;
/// One file-backed module, keyed by its `foo::bar` path (`<crate>` for root).
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct ModuleSizeInput {
    /// Module path (`foo::bar`, or `<crate>` for the root).
    path: String,
    /// Source file path, usually crate-relative.
    file: String,
    /// Line count of this file.
    #[getter(copy)]
    lines: u32,
}

/// A node in the module tree after order and subtree sizes are filled in.
#[derive(Debug, Clone, PartialEq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ModuleHierarchyNode {
    /// Module path (`foo::bar`, or `<crate>` for the root).
    path: String,
    /// Source file path, usually crate-relative.
    file: String,
    /// Lines in this module's own file.
    #[getter(copy)]
    own_lines: u32,
    /// Lines in this module and all descendants.
    #[getter(copy)]
    subtree_lines: u32,
    /// Horton–Strahler order of this module.
    #[getter(copy)]
    order: u32,
    /// Depth from the crate root (0 at `<crate>`).
    #[getter(copy)]
    depth: u32,
    /// Number of direct child modules.
    #[getter(copy)]
    child_count: usize,
    /// Direct child module paths (file modules only).
    children: Vec<String>,
}

impl ModuleHierarchyNode {
    /// Fraction of subtree lines that live in this module's own file.
    #[instrument(level = "debug", skip(self))]
    pub fn top_heavy(&self) -> f64 {
        if self.subtree_lines == 0 {
            0.0
        } else {
            f64::from(self.own_lines) / f64::from(self.subtree_lines)
        }
    }

    /// Whether this module has child modules.
    #[instrument(level = "trace", skip(self))]
    pub fn is_branch(&self) -> bool {
        self.child_count > 0
    }
}

pub const CRATE_ROOT_PATH: &str = "<crate>";

#[instrument(level = "debug", skip(path))]
fn parent_path(path: &str) -> Option<String> {
    if path == CRATE_ROOT_PATH {
        None
    } else if let Some((parent, _)) = path.rsplit_once("::") {
        Some(parent.to_string())
    } else {
        Some(CRATE_ROOT_PATH.to_string())
    }
}

#[instrument(level = "debug", skip(path, existing))]
fn nearest_parent(path: &str, existing: &BTreeSet<String>) -> Option<String> {
    let mut current = parent_path(path)?;
    loop {
        if existing.contains(&current) {
            return Some(current);
        }
        current = parent_path(&current)?;
    }
}

/// Build the module tree and assign Strahler order, depth, and subtree size.
#[instrument(level = "debug", skip(modules))]
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
#[instrument(level = "debug", skip(nodes))]
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
#[instrument(level = "debug", skip(nodes))]
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
#[instrument(level = "debug", skip(nodes))]
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
#[derive(Debug, Clone, PartialEq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct SiblingImbalance {
    /// Parent identifier, when this node is nested.
    parent: String,
    /// Child with the largest subtree.
    largest: String,
    /// Subtree line count of the largest child.
    #[getter(copy)]
    largest_subtree: u32,
    /// Combined subtree lines of all siblings.
    #[getter(copy)]
    sibling_total: u32,
    /// Largest child's fraction of sibling subtree lines.
    #[getter(copy)]
    share: f64,
    /// How many siblings were compared.
    #[getter(copy)]
    sibling_count: usize,
    /// Sibling module paths with their subtree line counts.
    siblings: Vec<(String, u32)>,
}

/// Parents with two or more children, ranked by the largest child's share of
/// sibling subtree lines. Children below `min_child_lines` are omitted from
/// the group (a stub next to a real module is not lopsided).
#[instrument(level = "debug", skip(nodes))]
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

/// A parent whose only child is itself a branch: an extra hop with no fork.
#[derive(Debug, Clone, PartialEq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct UnaryNest {
    /// Parent identifier, when this node is nested.
    parent: String,
    /// Unary nest whose only child is itself a branch.
    passthrough: String,
    /// Own-file lines of the passthrough module.
    #[getter(copy)]
    passthrough_own: u32,
    /// Subtree lines of the passthrough module.
    #[getter(copy)]
    passthrough_subtree: u32,
    /// Children of the passthrough's only child.
    grandchildren: Vec<(String, u32)>,
}

/// Parents (not the crate root) with exactly one child, when that child has
/// children of its own and a subtree of at least `min_subtree_lines`.
///
/// A unary *leaf* is a peel (`chain_layer` + `preds.rs`), not this. Ranked
/// by passthrough subtree, largest first.
#[instrument(level = "debug", skip(nodes))]
pub fn unary_nests(nodes: &[ModuleHierarchyNode], min_subtree_lines: u32) -> Vec<UnaryNest> {
    let by_path: HashMap<&str, &ModuleHierarchyNode> = nodes
        .iter()
        .map(|node| (node.path.as_str(), node))
        .collect();
    let mut ranked = Vec::new();
    for parent in nodes
        .iter()
        .filter(|node| node.path != CRATE_ROOT_PATH && node.children.len() == 1)
    {
        let Some(child) = by_path.get(parent.children[0].as_str()).copied() else {
            continue;
        };
        if !child.is_branch() || child.subtree_lines < min_subtree_lines {
            continue;
        }
        ranked.push(UnaryNest {
            parent: parent.path.clone(),
            passthrough: child.path.clone(),
            passthrough_own: child.own_lines,
            passthrough_subtree: child.subtree_lines,
            grandchildren: child
                .children
                .iter()
                .filter_map(|path| {
                    by_path
                        .get(path.as_str())
                        .map(|kid| (kid.path.clone(), kid.subtree_lines))
                })
                .collect(),
        });
    }
    ranked.sort_by(|left, right| {
        right
            .passthrough_subtree
            .cmp(&left.passthrough_subtree)
            .then_with(|| left.passthrough.cmp(&right.passthrough))
    });
    ranked
}

#[instrument(level = "debug")]
pub fn format_mass_list(entries: &[(String, u32)]) -> String {
    entries
        .iter()
        .map(|(path, lines)| format!("{path} {lines}"))
        .collect::<Vec<_>>()
        .join("; ")
}

#[instrument(level = "debug", skip(node, nodes))]
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
#[instrument(level = "debug", skip(nodes))]
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

/// A Strahler-order band used when grouping hierarchy findings.
#[derive(Debug, Clone, Copy, PartialEq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct OrderBand {
    /// Horton–Strahler order of this module.
    #[getter(copy)]
    order: u32,
    /// How many modules fall in this order band.
    #[getter(copy)]
    count: usize,
    /// Mean own-file lines in this band.
    #[getter(copy)]
    mean_own: f64,
    /// Mean subtree lines in this band.
    #[getter(copy)]
    mean_subtree: f64,
}
