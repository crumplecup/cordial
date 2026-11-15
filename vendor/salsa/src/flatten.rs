//! Flattening a query's dependencies down to what is persisted, once per query.
//!
//! A persisted memo's dependencies are rewritten before it is serialized: the queries that
//! are not themselves persisted are replaced by the persisted inputs they read (see
//! `Ingredient::collect_minimum_serialized_edges`). Walking those queries afresh for every
//! persisted memo costs the number of persisted memos times the size of the graph each one
//! reaches, and a rust-analyzer-sized graph is millions of queries. The set of persisted
//! edges a query reaches does not depend on who asks, so it is worked out once and kept.
//!
//! Queries that call each other (rust-analyzer has large groups of them) all reach the same
//! persisted edges, so they are worked out together: a walk is Tarjan's strongly connected
//! components algorithm, and when the first query of a group finishes, every member of the
//! group is given its result. A query reached again while its group is still open is skipped:
//! the group's first query collects what it reaches. Without that, a group of K queries that
//! all call each other is walked along every path through it, K! times.

use std::time::Instant;

use crate::durability::Durability;
use crate::hash::{FxHashMap, FxIndexSet};
use crate::sync::Arc;
use crate::zalsa_local::QueryEdge;

/// Inputs of this durability or higher are summarized by one edge in a persisted memo (see
/// [`QueryEdge::watermark`]): they are the library's and the crate graph's, thousands for a
/// memo that reads a definition map, and they change together and rarely.
pub(crate) const WATERMARK_DURABILITY: Durability = Durability::MEDIUM;

/// The persisted edges reached from each query that is not persisted, kept for one
/// ingredient's memos while they are serialized.
#[doc(hidden)]
pub struct EdgeFlattener {
    /// The finished result for a query: exact, whoever asked.
    done: FxHashMap<QueryEdge, Arc<[QueryEdge]>>,
    /// Queries walked whose group is still open, with the order they were first reached in.
    open: FxHashMap<QueryEdge, usize>,
    /// The open queries, in the order they were first reached.
    stack: Vec<QueryEdge>,
    /// The order the next query walked will be given.
    next: usize,
    /// The earliest open query reached from within the walk of the current query.
    lowlink: usize,
    /// The result of the query just walked, for the caller to take.
    last: Option<Arc<[QueryEdge]>>,
    started: Instant,
    stats: FlattenStats,
}

/// What a flattening did, for the log.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct FlattenStats {
    /// Queries walked.
    pub(crate) walked: u64,
    /// Queries answered from what was kept.
    pub(crate) reused: u64,
    /// Queries that finished while the group they belong to was still open.
    pub(crate) incomplete: u64,
    /// Results that were the same as one of their callees' and shared its storage.
    pub(crate) shared: u64,
    /// Edges handed to callers, counting repeats.
    pub(crate) handed_out: u64,
}

impl Default for EdgeFlattener {
    fn default() -> Self {
        Self {
            done: FxHashMap::default(),
            open: FxHashMap::default(),
            stack: Vec::new(),
            next: 0,
            lowlink: usize::MAX,
            last: None,
            started: Instant::now(),
            stats: FlattenStats::default(),
        }
    }
}

impl EdgeFlattener {
    /// The kept result for `edge`, if its walk finished.
    pub(crate) fn kept(&mut self, edge: QueryEdge) -> Option<Arc<[QueryEdge]>> {
        let result = self.done.get(&edge).cloned()?;
        self.stats.reused += 1;
        Some(result)
    }

    /// Whether `edge` was walked and its group is still open. If it was, whatever the
    /// walk of the query asking reaches is part of that group.
    pub(crate) fn reached_again(&mut self, edge: QueryEdge) -> bool {
        let Some(&order) = self.open.get(&edge) else {
            return false;
        };
        self.lowlink = self.lowlink.min(order);
        true
    }

    /// Start walking `edge`. Returns what [`Self::finish`] needs back.
    pub(crate) fn enter(&mut self, edge: QueryEdge) -> Walk {
        let order = self.next;
        self.next += 1;
        self.open.insert(edge, order);
        self.stack.push(edge);
        self.stats.walked += 1;
        Walk {
            order,
            outer: std::mem::replace(&mut self.lowlink, usize::MAX),
        }
    }

    /// Finish walking `edge`, whose persisted edges are `found`. `largest` is the biggest
    /// result among the queries it called: a result no bigger than that is that result.
    pub(crate) fn finish(
        &mut self,
        edge: QueryEdge,
        walk: Walk,
        found: FxIndexSet<QueryEdge>,
        largest: Option<Arc<[QueryEdge]>>,
    ) -> Arc<[QueryEdge]> {
        let result = match largest {
            Some(largest) if largest.len() == found.len() => {
                self.stats.shared += 1;
                largest
            }
            _ => found.into_iter().collect(),
        };
        if self.lowlink >= walk.order {
            // The first query of its group: everything the group reaches has been found, and
            // each member reaches exactly what the others do.
            while let Some(member) = self.stack.pop() {
                self.open.remove(&member);
                self.done.insert(member, Arc::clone(&result));
                if member == edge {
                    break;
                }
            }
            self.lowlink = walk.outer;
        } else {
            // A member of a group opened above it: its result is the group's, given when the
            // group's first query finishes.
            self.stats.incomplete += 1;
            self.lowlink = walk.outer.min(self.lowlink);
        }
        result
    }

    /// Say what the caller of the last walk gets.
    pub(crate) fn hand_out(&mut self, result: &Arc<[QueryEdge]>) {
        self.stats.handed_out += result.len() as u64;
        self.last = Some(Arc::clone(result));
    }

    /// The result of the query just walked, if there was one.
    pub(crate) fn take_last(&mut self) -> Option<Arc<[QueryEdge]>> {
        self.last.take()
    }

    /// What this has done so far, and for how long.
    pub(crate) fn report(&self) -> (FlattenStats, std::time::Duration) {
        (self.stats, self.started.elapsed())
    }
}

/// The state of a query's walk that must be given back when it finishes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Walk {
    order: usize,
    outer: usize,
}
