//! Flattening a query's dependencies down to what is persisted, once per query.
//!
//! A persisted memo's dependencies are rewritten before it is saved: the queries that
//! are not themselves persisted are replaced by the persisted inputs they read (see
//! `Ingredient::collect_minimum_serialized_edges`). Walking those queries afresh for every
//! persisted memo costs the number of persisted memos times the size of the graph each one
//! reaches, and a rust-analyzer-sized graph is millions of queries. The set of persisted
//! edges a query reaches does not depend on who asks, so it is worked out once and kept.
//!
//! The sets are bitsets over the edges seen so far, shared: two queries that reach the same
//! edges hold the same set, so a persisted memo can say "the edges of this set" instead of
//! listing them, and the file holds each distinct set once.
//!
//! Queries that call each other (rust-analyzer has large groups of them) all reach the same
//! persisted edges, so they are worked out together: a walk is Tarjan's strongly connected
//! components algorithm, and when the first query of a group finishes, every member of the
//! group is given its result. A query reached again while its group is still open is skipped:
//! the group's first query collects what it reaches. Without that, a group of K queries that
//! all call each other is walked along every path through it, K! times.

use std::cell::RefCell;
use std::collections::HashSet;
use std::time::Instant;

use crate::durability::Durability;
use crate::hash::{FxHashMap, FxIndexSet};
use crate::sync::Arc;
use crate::zalsa::Zalsa;
use crate::zalsa_local::QueryEdge;

/// Inputs of this durability or higher are summarized by one edge in a persisted memo (see
/// [`QueryEdge::watermark`]): they are the library's and the crate graph's, thousands for a
/// memo that reads a definition map, and they change together and rarely.
pub(crate) const WATERMARK_DURABILITY: Durability = Durability::MEDIUM;

static PERSISTED_EXECUTIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Record that a persisted query was executed: what is saved of the database is no longer
/// what was restored.
pub(crate) fn note_persisted_execution() {
    PERSISTED_EXECUTIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// How many persisted queries this process has executed (not restored, not reused), so that a
/// caller can tell whether anything has been computed that a saved state does not hold.
pub fn persisted_executions() -> u64 {
    PERSISTED_EXECUTIONS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Say, as an event, that a memo was found out of date because of the dependency `edge`: the
/// memo's query and key, and the input's field and key. One event for each memo, at the place it
/// is found, so that a log holds the answer to whatever is asked of it afterwards (the keys are
/// those of the structs, to be joined with a table of what they are).
pub(crate) fn stale_event(zalsa: &Zalsa, edge: QueryEdge, memo: crate::key::DatabaseKeyIndex) {
    ::tracing::debug!(
        target: "salsa::stale",
        memo = zalsa.lookup_ingredient(memo.ingredient_index()).debug_name(),
        memo_key = memo.key_index().as_bits(),
        changed = if edge.is_watermark() {
            "<watermark>"
        } else {
            zalsa.lookup_ingredient(edge.key().ingredient_index()).debug_name()
        },
        changed_key = edge.key().key_index().as_bits(),
        "a memo is out of date"
    );
}

/// A set of persisted edges, as the bits of the edges' numbers in one [`EdgeFlattener`].
#[derive(PartialEq, Eq, Hash)]
pub(crate) struct Bits(Box<[u64]>);

/// A set shared by every query that reaches exactly these edges.
pub(crate) type LeafSet = Arc<Bits>;

/// A set being built.
#[derive(Default)]
pub(crate) struct Builder(Vec<u64>);

impl Builder {
    fn set(&mut self, bit: u32) {
        let (word, bit) = ((bit / 64) as usize, bit % 64);
        if self.0.len() <= word {
            self.0.resize(word + 1, 0);
        }
        self.0[word] |= 1 << bit;
    }

    fn or(&mut self, other: &Bits) {
        if self.0.len() < other.0.len() {
            self.0.resize(other.0.len(), 0);
        }
        for (mine, theirs) in self.0.iter_mut().zip(other.0.iter()) {
            *mine |= theirs;
        }
    }
}

/// The persisted edges reached from each query that is not persisted, kept for one
/// ingredient's memos while they are serialized.
#[doc(hidden)]
pub struct EdgeFlattener {
    /// The number of each persisted edge seen, and the edge of each number.
    numbers: FxHashMap<QueryEdge, u32>,
    edges: Vec<QueryEdge>,
    /// Every distinct set, so that equal sets are the same set.
    interned: HashSet<LeafSet>,
    /// The finished result for a query: exact, whoever asked.
    done: FxHashMap<QueryEdge, LeafSet>,
    /// Queries walked whose group is still open, with the order they were first reached in.
    open: FxHashMap<QueryEdge, usize>,
    /// The open queries, in the order they were first reached.
    stack: Vec<QueryEdge>,
    /// The order the next query walked will be given.
    next: usize,
    /// The earliest open query reached from within the walk of the current query.
    lowlink: usize,
    /// Edges a hook adds, taken to be put in a set.
    scratch: FxIndexSet<QueryEdge>,
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
    /// Distinct sets made.
    pub(crate) sets: u64,
}

impl Default for EdgeFlattener {
    fn default() -> Self {
        Self {
            numbers: FxHashMap::default(),
            edges: Vec::new(),
            interned: HashSet::new(),
            done: FxHashMap::default(),
            open: FxHashMap::default(),
            stack: Vec::new(),
            next: 0,
            lowlink: usize::MAX,
            scratch: FxIndexSet::default(),
            started: Instant::now(),
            stats: FlattenStats::default(),
        }
    }
}

impl EdgeFlattener {
    fn number(&mut self, edge: QueryEdge) -> u32 {
        if let Some(&number) = self.numbers.get(&edge) {
            return number;
        }
        let number = u32::try_from(self.edges.len()).expect("fewer than 2^32 persisted edges");
        self.numbers.insert(edge, number);
        self.edges.push(edge);
        number
    }

    /// The set of exactly the edges in `builder`, shared with any equal set made before.
    fn intern(&mut self, builder: Builder) -> LeafSet {
        let mut words = builder.0;
        while words.last() == Some(&0) {
            words.pop();
        }
        let bits = Bits(words.into_boxed_slice());
        if let Some(existing) = self.interned.get(&bits) {
            return Arc::clone(existing);
        }
        self.stats.sets += 1;
        let set = Arc::new(bits);
        self.interned.insert(Arc::clone(&set));
        set
    }

    /// The edges in `set`.
    pub(crate) fn edges_of<'a>(&'a self, set: &'a Bits) -> impl Iterator<Item = QueryEdge> + 'a {
        set.0.iter().enumerate().flat_map(move |(index, word)| {
            let mut word = *word;
            std::iter::from_fn(move || {
                if word == 0 {
                    return None;
                }
                let bit = word.trailing_zeros();
                word &= word - 1;
                Some(self.edges[index * 64 + bit as usize])
            })
        })
    }

    /// What this has done so far, and for how long.
    pub(crate) fn report(&self) -> (FlattenStats, std::time::Duration) {
        (self.stats, self.started.elapsed())
    }
}

/// The persisted edges the function memo at `edge` reaches, or `None` if there is no memo
/// (or it is being walked: see the module's note on groups).
///
/// # Safety
///
/// The caller must prevent database writes throughout the traversal, including query
/// execution that can reuse slots or modify memos. Concurrent read-only access is allowed.
pub(crate) unsafe fn leaves_of(
    zalsa: &Zalsa,
    edge: QueryEdge,
    flattener: &mut EdgeFlattener,
) -> Option<LeafSet> {
    // What this query reaches does not depend on who asks: it is worked out once.
    if let Some(result) = flattener.done.get(&edge) {
        flattener.stats.reused += 1;
        return Some(Arc::clone(result));
    }

    // A query whose group is still open: its group's first query collects the rest.
    if let Some(&order) = flattener.open.get(&edge) {
        flattener.lowlink = flattener.lowlink.min(order);
        return None;
    }

    let ingredient = zalsa.lookup_ingredient(edge.key().ingredient_index());
    // SAFETY: The caller prevents database writes throughout the traversal. The checked
    // accessor would refuse the memos of interned keys that have not been used lately.
    let memo = unsafe {
        ingredient
            .as_function()?
            .memo_unchecked(zalsa, edge.key().key_index())
    }?;

    let order = flattener.next;
    flattener.next += 1;
    flattener.open.insert(edge, order);
    flattener.stack.push(edge);
    flattener.stats.walked += 1;
    let outer = std::mem::replace(&mut flattener.lowlink, usize::MAX);

    let mut found = Builder::default();
    for dependency_edge in memo.header().origin().edges() {
        if dependency_edge.is_watermark() {
            let number = flattener.number(dependency_edge);
            found.set(number);
            continue;
        }
        let dependency = zalsa.lookup_ingredient(dependency_edge.key().ingredient_index());

        if dependency.as_function().is_some() {
            if dependency.is_persistable() {
                // A persisted query is serialized itself: depend on it, and do not walk through
                // it (its own dependencies are covered by its own memo).
                let number = flattener.number(dependency_edge);
                found.set(number);
            } else if let Some(set) = unsafe { leaves_of(zalsa, dependency_edge, flattener) } {
                found.or(&set);
            }
            continue;
        }

        let mut added = std::mem::take(&mut flattener.scratch);
        // SAFETY: The caller prevents database writes throughout recursive traversal.
        unsafe {
            dependency.collect_minimum_serialized_edges(
                zalsa,
                dependency_edge,
                &mut added,
                flattener,
            );
        }
        for leaf in added.drain(..) {
            let number = flattener.number(leaf);
            found.set(number);
        }
        flattener.scratch = added;
    }

    let result = flattener.intern(found);
    if flattener.lowlink >= order {
        // The first query of its group: everything the group reaches has been found, and
        // each member reaches exactly what the others do.
        while let Some(member) = flattener.stack.pop() {
            flattener.open.remove(&member);
            flattener.done.insert(member, Arc::clone(&result));
            if member == edge {
                break;
            }
        }
        flattener.lowlink = outer;
    } else {
        // A member of a group opened above it: its result is the group's, given when the
        // group's first query finishes.
        flattener.stats.incomplete += 1;
        flattener.lowlink = outer.min(flattener.lowlink);
    }
    Some(result)
}

thread_local! {
    /// The sets the ingredient being deserialized saved, for the memos that refer to them.
    static SAVED_SETS: RefCell<Vec<Box<[QueryEdge]>>> = const { RefCell::new(Vec::new()) };
}

/// Hold the sets a saved ingredient listed while its memos are deserialized.
#[cfg(feature = "persistence")]
pub(crate) struct SavedSets;

#[cfg(feature = "persistence")]
impl SavedSets {
    pub(crate) fn install(sets: Vec<Box<[QueryEdge]>>) -> Self {
        SAVED_SETS.with(|saved| *saved.borrow_mut() = sets);
        Self
    }
}

#[cfg(feature = "persistence")]
impl Drop for SavedSets {
    fn drop(&mut self) {
        SAVED_SETS.with(|saved| saved.borrow_mut().clear());
    }
}

/// The edges of the saved sets `sets` and `edges`, each once, in that order.
#[cfg(feature = "persistence")]
pub(crate) fn expand_saved(sets: &[u32], edges: &[QueryEdge]) -> Box<[QueryEdge]> {
    SAVED_SETS.with(|saved| {
        let saved = saved.borrow();
        let mut all = FxIndexSet::default();
        for set in sets {
            let set = saved
                .get(*set as usize)
                .unwrap_or_else(|| panic!("a memo refers to set {set}, which was not saved"));
            all.extend(set.iter().copied());
        }
        all.extend(edges.iter().copied());
        all.into_iter().collect()
    })
}


/// Whether `edge` is a crate's `data` that survived flattening: not the watermark, so an input
/// of the lowest durability. Crates the loader made are written at a higher one, so these are
/// the ones a tool added.
pub(crate) fn is_low_crate_data(zalsa: &Zalsa, edge: QueryEdge) -> bool {
    if edge.is_watermark() {
        return false;
    }
    let ingredient = zalsa.lookup_ingredient(edge.key().ingredient_index());
    ingredient.debug_name() == "data" && !ingredient.watermarked(zalsa, edge.key().key_index())
}

/// The chain of queries from the memo whose edges are `edges` down to the first crate `data`
/// input of the lowest durability it depends on, as text for an event.
///
/// # Safety
///
/// The caller must prevent database writes throughout the traversal.
pub(crate) unsafe fn explain_low_crate_data(
    zalsa: &Zalsa,
    edges: crate::zalsa_local::QueryEdges<'_>,
) -> Option<String> {
    fn walk(
        zalsa: &Zalsa,
        edges: crate::zalsa_local::QueryEdges<'_>,
        seen: &mut std::collections::HashSet<QueryEdge>,
        path: &mut Vec<String>,
    ) -> bool {
        for edge in edges {
            if edge.is_watermark() || !seen.insert(edge) {
                continue;
            }
            let ingredient = zalsa.lookup_ingredient(edge.key().ingredient_index());
            let id = edge.key().key_index();
            if is_low_crate_data(zalsa, edge) {
                path.push(format!("{}(#{})", ingredient.debug_name(), id.as_bits()));
                return true;
            }
            let Some(function) = ingredient.as_function() else {
                continue;
            };
            // SAFETY: The caller prevents database writes.
            let Some(memo) = (unsafe { function.memo_unchecked(zalsa, id) }) else {
                continue;
            };
            path.push(format!("{}(#{})", ingredient.debug_name(), id.as_bits()));
            if walk(zalsa, memo.header().origin().edges(), seen, path) {
                return true;
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    walk(zalsa, edges, &mut std::collections::HashSet::new(), &mut path).then(|| path.join(" -> "))
}
