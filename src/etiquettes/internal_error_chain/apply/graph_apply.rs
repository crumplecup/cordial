//! Whole-file plans for a private error, its Kind, and native source fields.
//!
//! The planner uses the shared parsed tree. A graph is edited only when its
//! declarations, construction expressions, and storage-sensitive reads all
//! have a supported rewrite.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use syn::visit::Visit;
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::error::{CordialError, CordialResult};
use crate::ir::{IrMut, IrView};
use crate::loader::SharedParsedFile;

use super::Edit;
use super::inventory::{CrateTypeIndex, ErrorEdgeTarget};

mod distributed;
mod expressions;
mod lifetimes;
mod plan;
mod types;

use expressions::GraphExprPlanner;
use plan::plan_file;

const GRAPH_ATTR: &str = "error_apply_graph_plans";

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(super) struct ErrorGraphPlan {
    path: PathBuf,
    #[getter(copy)]
    source_digest: [u8; 32],
    #[getter(skip)]
    names: BTreeSet<String>,
    fields: Vec<(String, String)>,
    component_paths: BTreeSet<PathBuf>,
    #[getter(skip)]
    edits: Option<Vec<Edit>>,
}

struct GraphCandidate {
    plan: ErrorGraphPlan,
    changes: BTreeMap<(String, String), Change>,
    variants: BTreeMap<(String, String), VariantChange>,
    constructed: BTreeSet<(String, String)>,
    constructed_variants: BTreeSet<(String, String)>,
    lifetime_transition: bool,
}

impl ErrorGraphPlan {
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn edits(&self) -> Option<&[Edit]> {
        self.edits.as_deref()
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn covers(&self, name: &str) -> bool {
        self.names.contains(name)
    }
}

#[instrument(level = "debug", skip(ir, files, strategy))]
pub(super) fn enrich_conversion_graphs(
    ir: &mut dyn IrMut,
    files: &[SharedParsedFile],
    strategy: ErrorFieldStrategy,
) -> CordialResult<()> {
    let index = CrateTypeIndex::from_ir(ir)?;
    let mut candidates: Vec<_> = files
        .iter()
        .filter_map(|file| plan_file(file, strategy, &index))
        .collect();
    let mut plans = Vec::new();
    let mut distributed_paths = BTreeSet::new();
    for file in files {
        if distributed_paths.contains(file.path()) {
            continue;
        }
        if let Some(group) = distributed::plan(file, files, strategy, &index) {
            distributed_paths.extend(group.iter().map(|plan| plan.path.clone()));
            plans.extend(group);
        }
    }
    candidates.retain(|candidate| !distributed_paths.contains(candidate.plan.path()));
    if !candidates.is_empty() {
        for mut candidate in candidates.drain(..) {
            let mut external = Vec::new();
            let expected: BTreeMap<_, ErrorEdgeTarget> = candidate
                .plan
                .names
                .iter()
                .filter_map(|name| {
                    let home = files
                        .iter()
                        .find(|file| file.path() == candidate.plan.path())?;
                    index
                        .resolve(home, &[name.clone()])
                        .map(|target| (name.clone(), target))
                })
                .collect();
            let mut valid = candidate.plan.edits.is_some();
            for file in files {
                if file.path() == candidate.plan.path()
                    || !candidate
                        .plan
                        .names
                        .iter()
                        .any(|name| index.file_mentions(file, name))
                {
                    continue;
                }
                if candidate.lifetime_transition
                    || expected.len() != candidate.plan.names.len()
                    || !index.paths_resolve_to_graph(file, &expected)
                    || index.name_blocks(file, "Box")
                {
                    valid = false;
                    break;
                }
                let mut visitor =
                    GraphExprPlanner::new(file, strategy, &candidate.changes, &candidate.variants)
                        .strict_external();
                visitor.visit_file(file.syntax());
                let (edits, complete, constructed, variants) = visitor.finish();
                valid &= complete;
                candidate.constructed.extend(constructed);
                candidate.constructed_variants.extend(variants);
                if !edits.is_empty() {
                    external.push(ErrorGraphPlan {
                        path: file.path().to_path_buf(),
                        source_digest: file.source_digest(),
                        names: BTreeSet::new(),
                        fields: Vec::new(),
                        component_paths: BTreeSet::new(),
                        edits: Some(edits),
                    });
                }
            }
            valid &= candidate
                .changes
                .keys()
                .all(|key| candidate.constructed.contains(key));
            valid &= candidate
                .variants
                .keys()
                .all(|key| candidate.constructed_variants.contains(key));
            if !valid {
                candidate.plan.edits = None;
                external.clear();
            } else if !external.is_empty() {
                let component_paths: BTreeSet<_> = std::iter::once(candidate.plan.path.clone())
                    .chain(external.iter().map(|plan| plan.path.clone()))
                    .collect();
                candidate.plan.component_paths = component_paths.clone();
                for plan in &mut external {
                    plan.component_paths = component_paths.clone();
                }
            }
            plans.push(candidate.plan);
            plans.extend(external);
        }
    }
    let mut by_path: BTreeMap<PathBuf, ErrorGraphPlan> = BTreeMap::new();
    for plan in plans {
        if let Some(existing) = by_path.get_mut(plan.path()) {
            existing.names.extend(plan.names);
            existing.fields.extend(plan.fields);
            existing.component_paths.extend(plan.component_paths);
            existing.edits = match (existing.edits.take(), plan.edits) {
                (Some(mut left), Some(right)) => {
                    left.extend(right);
                    Some(left)
                }
                _ => None,
            };
        } else {
            by_path.insert(plan.path.clone(), plan);
        }
    }
    let root = ir.root()?;
    ir.set_attr(
        root,
        GRAPH_ATTR,
        serde_json::to_value(by_path.into_values().collect::<Vec<_>>())?,
    )?;
    Ok(())
}

#[instrument(level = "debug", skip(ir))]
pub(super) fn conversion_graphs_from_ir(ir: &dyn IrView) -> CordialResult<Vec<ErrorGraphPlan>> {
    let root = ir.root()?;
    let value = ir
        .node(root)
        .and_then(|node| node.attr(GRAPH_ATTR).cloned())
        .ok_or_else(|| CordialError::unreachable(GRAPH_ATTR))?;
    Ok(serde_json::from_value(value)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    String,
    Value,
    Boxed,
}

#[derive(Debug, Clone)]
struct Change {
    shape: Shape,
}

#[derive(Debug, Clone)]
struct VariantChange {
    shape: Shape,
}
