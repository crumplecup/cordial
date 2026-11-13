//! Which functions exist only in a verifier's build, from the call graph.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::graph::{Dispatch, Graph, Origin};

/// The vertices that are never worth instrumenting, as a set of vertex indexes.
///
/// A function is a seed when it exists only in a verifier's build: it is inactive
/// in the ordinary build and active only in contexts that turn on one of its
/// crate's gate cfgs, or a macro-read function names one in its gates. The rest
/// follow by reachability: a function with at least one caller, every one of
/// which is excluded, is as dead to tracing as they are. A function with no
/// known caller is never added (public API, or dead code), and neither is one
/// whose callers cannot all be seen.
#[tracing::instrument(level = "debug", skip(graph, gates))]
#[hotpath::measure]
pub(super) fn proof_only(
    graph: &Graph,
    gates: &HashMap<String, BTreeSet<String>>,
) -> HashSet<usize> {
    let mut excluded: HashSet<usize> = (0..graph.vertices().len())
        .filter(|vertex| is_seed(graph, *vertex, gates))
        .collect();
    loop {
        let mut added = false;
        for vertex in 0..graph.vertices().len() {
            if excluded.contains(&vertex)
                || matches!(graph.vertices()[vertex].dispatch(), Dispatch::Foreign)
            {
                continue;
            }
            let callers = graph.callers(vertex);
            if !callers.is_empty() && callers.iter().all(|caller| excluded.contains(caller)) {
                excluded.insert(vertex);
                added = true;
            }
        }
        if !added {
            return excluded;
        }
    }
}

#[tracing::instrument(level = "trace", skip(graph, gates))]
fn is_seed(graph: &Graph, vertex: usize, gates: &HashMap<String, BTreeSet<String>>) -> bool {
    let vertex = &graph.vertices()[vertex];
    let Some(gates) = gates
        .get(vertex.crate_name())
        .filter(|gates| !gates.is_empty())
    else {
        return false;
    };
    match vertex.origin() {
        Origin::Island(names) => names.iter().any(|name| gates.contains(name)),
        Origin::Source(contexts) => {
            !contexts.is_empty()
                && !contexts.iter().any(|context| context.is_ordinary())
                && contexts
                    .iter()
                    .all(|context| context.enabled().iter().any(|atom| gates.contains(atom)))
        }
    }
}
