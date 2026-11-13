//! The workspace's functions and who calls them, read from the crate IRs.
//!
//! Every edge is a resolved fact the loader recorded: a `Calls` or `ContextCalls`
//! edge inside a crate, an external call matched to its callee by source
//! location, or a call to a trait declaration followed to each implementation.

use std::collections::{BTreeSet, HashMap};

use crate::ir::SemanticContextKey;
use crate::{
    BasicQuery, CordialResult, CrateIr, EdgeKind, FunctionLocation, IrView, ItemKind, NodeId,
    NodeKind,
};

/// Where a function's definition comes from.
#[derive(Debug, Clone)]
pub(super) enum Origin {
    /// A definition rust-analyzer resolved, active in these cfg contexts.
    Source(Vec<SemanticContextKey>),
    /// Read from a macro invocation's tokens, gated by these names.
    Island(Vec<String>),
}

/// How the callers of a function are found.
#[derive(Debug, Clone)]
pub(super) enum Dispatch {
    /// Only calls that name the function reach it.
    Direct,
    /// It implements these trait declarations, so calls to them reach it too.
    Implements(Vec<usize>),
    /// It implements a trait the workspace does not own: not every caller is visible.
    Foreign,
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub(super) struct Vertex {
    crate_name: String,
    qualified_name: String,
    origin: Origin,
    dispatch: Dispatch,
}

/// Every function of the workspace and the functions that call each.
#[derive(Debug, Default, derive_getters::Getters)]
pub(super) struct Graph {
    vertices: Vec<Vertex>,
    direct_callers: Vec<BTreeSet<usize>>,
}

impl Graph {
    /// Callers of `vertex`, including the callers of each trait declaration it
    /// implements: a call through a trait runs some implementation.
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn callers(&self, vertex: usize) -> BTreeSet<usize> {
        let mut callers = self.direct_callers[vertex].clone();
        if let Dispatch::Implements(declarations) = self.vertices[vertex].dispatch() {
            for declaration in declarations {
                callers.extend(&self.direct_callers[*declaration]);
            }
        }
        callers.remove(&vertex);
        callers
    }
}

/// Read the graph of every crate in `crates`.
#[tracing::instrument(level = "debug", skip(crates), err(level = "warn"))]
#[hotpath::measure]
pub(super) fn build(crates: &HashMap<String, CrateIr>) -> CordialResult<Graph> {
    let mut names: Vec<&String> = crates.keys().collect();
    names.sort();
    let mut nodes: Vec<(&str, NodeId, Option<FunctionLocation>)> = Vec::new();
    let mut origins: Vec<(String, Origin)> = Vec::new();
    for name in names {
        let ir = &crates[name];
        let activity = ir.context_activity()?;
        for node in ir.nodes_matching(&BasicQuery::items()) {
            if node.kind() != &NodeKind::Item(ItemKind::Fn) {
                continue;
            }
            let Some(qualified) = node.attr("qualified_path").and_then(|path| path.as_str()) else {
                continue;
            };
            let origin = match ir.macro_island(node.id())? {
                Some(island) => Origin::Island(island.gates().clone()),
                None => Origin::Source(match &activity {
                    Some(activity) => activity.active(ir, node.id())?,
                    None => Vec::new(),
                }),
            };
            nodes.push((name, node.id(), ir.function_location(node.id())?));
            origins.push((qualified.to_owned(), origin));
        }
    }
    let mut index: HashMap<(&str, NodeId), usize> = HashMap::new();
    let mut by_location: HashMap<&FunctionLocation, Vec<usize>> = HashMap::new();
    for (vertex, (name, node, location)) in nodes.iter().enumerate() {
        index.insert((name, *node), vertex);
        if let Some(location) = location {
            by_location.entry(location).or_default().push(vertex);
        }
    }
    let mut vertices = Vec::with_capacity(nodes.len());
    let mut direct_callers = vec![BTreeSet::new(); nodes.len()];
    for (vertex, ((name, node, _), (qualified, origin))) in nodes.iter().zip(origins).enumerate() {
        let ir = &crates[*name];
        for kind in [EdgeKind::Calls, EdgeKind::ContextCalls] {
            for (_, callee) in ir.neighbors(*node, kind, petgraph::Direction::Outgoing) {
                if let Some(callee) = index.get(&(*name, callee)) {
                    direct_callers[*callee].insert(vertex);
                }
            }
        }
        for external in ir.external_calls(*node)? {
            for callee in by_location.get(external.callee()).into_iter().flatten() {
                direct_callers[*callee].insert(vertex);
            }
        }
        let dispatch = match ir.trait_implementation(*node)? {
            None => Dispatch::Direct,
            Some(implementation) => match implementation
                .declaration()
                .as_ref()
                .and_then(|location| by_location.get(location))
            {
                Some(declarations) => Dispatch::Implements(declarations.clone()),
                None => Dispatch::Foreign,
            },
        };
        vertices.push(Vertex::new((*name).to_owned(), qualified, origin, dispatch));
    }
    Ok(Graph {
        vertices,
        direct_callers,
    })
}
