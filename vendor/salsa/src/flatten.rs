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

use std::collections::HashSet;

use crate::durability::Durability;
use crate::hash::{FxHashMap, FxIndexSet};
use crate::sync::Arc;
use crate::zalsa::Zalsa;
use crate::zalsa_local::QueryEdge;

/// Inputs of this durability or higher are summarized by one edge in a persisted memo (see
/// [`QueryEdge::watermark`]): they are the library's and the crate graph's, thousands for a
/// memo that reads a definition map, and they change together and rarely.
pub(crate) const WATERMARK_DURABILITY: Durability = Durability::HIGH;

static PERSISTED_EXECUTIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Record that a persisted query was executed: what is saved of the database is no longer
/// what was restored.
pub(crate) fn note_persisted_execution() {
    PERSISTED_EXECUTIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

static EDGES_EXPANDED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many dependency edges deserializing saved memos has given memos of their own, one copy per
/// memo of every set it refers to. A memo in a crate of `n` files has `n` of them, so the total
/// is the number of memos times the files they depend on: the cost of a restore that no byte of
/// the saved state accounts for.
pub fn edges_expanded() -> u64 {
    EDGES_EXPANDED.load(std::sync::atomic::Ordering::Relaxed)
}

static DEEP_VERIFICATIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static EDGES_CHECKED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Record that a memo was deep-verified, with the number of dependencies it has to check.
pub(crate) fn note_deep_verification(edges: usize) {
    DEEP_VERIFICATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    EDGES_CHECKED.fetch_add(edges as u64, std::sync::atomic::Ordering::Relaxed);
}

/// How many memos this process has deep-verified (found out of date or not by walking their
/// dependencies) and how many dependencies those had: the time a restored run spends
/// re-validating what was saved, which no executed query accounts for.
pub fn deep_verifications() -> (u64, u64) {
    (
        DEEP_VERIFICATIONS.load(std::sync::atomic::Ordering::Relaxed),
        EDGES_CHECKED.load(std::sync::atomic::Ordering::Relaxed),
    )
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

/// A set of persisted edges, as the runs of consecutive numbers the edges have in one
/// [`EdgeFlattener`]: `(first, how many)`, sorted, and neither overlapping nor adjacent, so a set has
/// one form and equal sets are equal runs. The edges of a crate's files are numbered one after another
/// as they are first reached, so a set is a few long runs, and it takes what it holds and not how
/// wide the universe it is among is: a set of one edge numbered a million is one run.
#[doc(hidden)]
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Runs(Box<[(u32, u32)]>);

impl Runs {
    /// The runs, in order.
    pub fn runs(&self) -> &[(u32, u32)] {
        &self.0
    }

    /// How many numbers are in the set.
    pub fn len(&self) -> u64 {
        self.0.iter().map(|(_, length)| u64::from(*length)).sum()
    }

    /// Whether the set has no numbers.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The numbers in the set, in order.
    pub fn numbers(&self) -> impl Iterator<Item = u32> + '_ {
        self.0.iter().flat_map(|(first, length)| *first..*first + *length)
    }
}

/// A set shared by every query that reaches exactly these edges.
pub(crate) type LeafSet = Arc<Runs>;

/// What the queries a walk passes through say about the set it found: the latest revision any of
/// them changed at, and whether the walk can be trusted to have seen everything they read.
///
/// A persisted memo verified at revision `r` is still valid when none of this has changed since:
/// no input in its set (checked once for all the memos that share the set) and none of the queries
/// between it and the inputs (`changed` is not after `r`). A query between that is out of date
/// has an input in the set that changed, so it is not missed. A walk that met a query it cannot
/// read as a plain derived one (an untracked read, a value assigned by another query, no memo at
/// all where one was read) is not trusted, and the memo is verified by walking what it reads.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Closure {
    pub(crate) changed: crate::Revision,
    pub(crate) trusted: bool,
}

impl Closure {
    /// The closure of nothing: it has changed at no revision and has met nothing wrong.
    pub(crate) fn nothing() -> Self {
        Self {
            changed: crate::Revision::start(),
            trusted: true,
        }
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.changed = self.changed.max(other.changed);
        self.trusted &= other.trusted;
    }

}

/// A set being built: the runs of the numbers put in it and of the sets joined to it, in no order and
/// possibly overlapping, put in order and merged when the set is asked for and now and then while it
/// grows, so that joining the same large set many times does not hold it many times.
#[doc(hidden)]
#[derive(Default)]
pub struct RunBuilder {
    runs: Vec<(u32, u32)>,
    /// How many runs there were when they were last put in order.
    ordered: usize,
}

impl RunBuilder {
    /// Put `number` in the set.
    pub fn set(&mut self, number: u32) {
        match self.runs.last_mut() {
            Some((first, length)) if *first + *length == number => *length += 1,
            _ => self.runs.push((number, 1)),
        }
    }

    /// Put every number of `other` in the set.
    pub fn or(&mut self, other: &Runs) {
        self.runs.extend_from_slice(&other.0);
        if self.runs.len() > 64 && self.runs.len() > 2 * self.ordered {
            self.normalize();
        }
    }

    /// The runs in order, overlapping and adjacent ones merged.
    fn normalize(&mut self) {
        self.runs.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(self.runs.len());
        for &(first, length) in &self.runs {
            match merged.last_mut() {
                Some((before, span)) if u64::from(first) <= u64::from(*before) + u64::from(*span) => {
                    let end = (u64::from(first) + u64::from(length))
                        .max(u64::from(*before) + u64::from(*span));
                    *span = u32::try_from(end - u64::from(*before)).expect("fewer than 2^32 edges");
                }
                _ => merged.push((first, length)),
            }
        }
        self.ordered = merged.len();
        self.runs = merged;
    }

    /// The set of exactly the numbers put in.
    pub fn build(mut self) -> Runs {
        self.normalize();
        Runs(self.runs.into_boxed_slice())
    }
}

/// The persisted edges reached from each query that is not persisted, kept while the memos of one ingredient
/// are serialized, or of every ingredient but the fingerprints while a save lasts (what a query reaches
/// does not depend on which ingredient asks).
#[doc(hidden)]
pub struct EdgeFlattener {
    /// The number of each persisted edge seen, and the edge of each number.
    numbers: FxHashMap<QueryEdge, u32>,
    edges: Vec<QueryEdge>,
    /// Every distinct set, so that equal sets are the same set.
    interned: HashSet<LeafSet>,
    /// The finished result for a query: exact, whoever asked, with what its closure says.
    done: FxHashMap<QueryEdge, (LeafSet, Closure)>,
    /// Whether a walk has met a query it expected a memo for and found none.
    pub(crate) met_no_memo: bool,
    /// What the queries reached through a dependency on a tracked struct say, until the memo or
    /// query that depended on it takes them into its own closure.
    pub(crate) carried: Closure,
    /// The ingredient whose memos are being flattened, which a fingerprint's own dependencies do
    /// not stop short of the query it fingerprints for.
    pub(crate) owner: Option<crate::IngredientIndex>,
    /// For each kind of query the walk passed through: how many were walked, the edges their sets
    /// reached added up, and the most any one reached. Which kinds make the memos that read them
    /// depend on a great deal, and whether it is all of them or one.
    breadth: FxHashMap<&'static str, (u32, u64, u32)>,
    /// For each kind of query the walk passed through: how many were walked, the leaf edges they
    /// read themselves (not through a query below them) added up, and the most any one read. A kind
    /// that reads a great many itself is one that goes through every file of a crate.
    direct: FxHashMap<&'static str, (u32, u64, u32)>,
    /// For each kind of query: which kinds of leaf (a file's text, a source root, a crate's data)
    /// it read itself, and how many in all.
    direct_kinds: FxHashMap<(&'static str, &'static str), u64>,
    /// For each kind of query: its widest query's size, and which kinds of leaf that one reached,
    /// most first. Says what a query that reaches a great deal reaches (every file's text, every
    /// crate's fingerprint), which the sizes alone do not.
    widest_mix: FxHashMap<&'static str, (u32, String)>,
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
    /// The set of each restored set a memo refers to, made once.
    restored: FxHashMap<u32, LeafSet>,
    stats: FlattenStats,
}

/// What an [`EdgeFlattener`] holds, for the log.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    /// The persisted edges numbered: a set is a set of positions among these.
    pub universe: u64,
    /// The distinct sets.
    pub sets: u64,
    /// The runs the distinct sets hold between them, each 8 bytes. A set held as the bits of its
    /// numbers took a word for every 64 up to its highest, however few it held.
    pub runs: u64,
    /// The most runs any one set holds.
    pub widest_runs: u64,
    /// The queries whose result is kept, each with a set and a closure.
    pub kept: u64,
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
            met_no_memo: false,
            carried: Closure::nothing(),
            owner: None,
            breadth: FxHashMap::default(),
            direct: FxHashMap::default(),
            direct_kinds: FxHashMap::default(),
            widest_mix: FxHashMap::default(),
            open: FxHashMap::default(),
            stack: Vec::new(),
            next: 0,
            lowlink: usize::MAX,
            scratch: FxIndexSet::default(),
            restored: FxHashMap::default(),
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
    fn intern(&mut self, builder: RunBuilder) -> LeafSet {
        let bits = builder.build();
        if let Some(existing) = self.interned.get(&bits) {
            return Arc::clone(existing);
        }
        self.stats.sets += 1;
        let set = Arc::new(bits);
        self.interned.insert(Arc::clone(&set));
        set
    }

    /// The set of the edges of the restored set `number`: the set a memo that refers to it is saved
    /// as referring to, so a state saved again shares what the state restored shared.
    pub(crate) fn restored_set(&mut self, number: u32) -> LeafSet {
        if let Some(set) = self.restored.get(&number) {
            return Arc::clone(set);
        }
        let mut builder = RunBuilder::default();
        for edge in shared_set(number).edges() {
            let bit = self.number(edge);
            builder.set(bit);
        }
        let set = self.intern(builder);
        self.restored.insert(number, Arc::clone(&set));
        set
    }

    /// What an ingredient saves of `sets` when the flattener numbers edges for every ingredient of a
    /// save: the edges they hold, in the order they are numbered, and each set as runs of positions in
    /// that list. A set's runs are positions among the edges the ingredient's sets hold and not among all
    /// the edges there are, so what is saved of an ingredient does not grow with the others.
    pub(crate) fn localize(&self, sets: &[LeafSet]) -> (Vec<QueryEdge>, Vec<Vec<(u32, u32)>>) {
        let mut used = RunBuilder::default();
        for set in sets {
            used.or(set);
        }
        let used = used.build();
        let mut before: Vec<u32> = Vec::with_capacity(used.runs().len());
        let mut total = 0_u32;
        for (_, length) in used.runs() {
            before.push(total);
            total += length;
        }
        let universe: Vec<QueryEdge> = used
            .numbers()
            .map(|number| self.edges[number as usize])
            .collect();
        let table = sets
            .iter()
            .map(|set| {
                let mut runs: Vec<(u32, u32)> = Vec::with_capacity(set.runs().len());
                for (first, length) in set.runs() {
                    let at = used.runs().partition_point(|(start, _)| start <= first) - 1;
                    let local = before[at] + (first - used.runs()[at].0);
                    match runs.last_mut() {
                        Some((start, span)) if *start + *span == local => *span += length,
                        _ => runs.push((local, *length)),
                    }
                }
                runs
            })
            .collect();
        (universe, table)
    }

    /// The edges in `set`.
    pub(crate) fn edges_of<'a>(&'a self, set: &'a Runs) -> impl Iterator<Item = QueryEdge> + 'a {
        set.numbers().map(|number| self.edges[number as usize])
    }

    /// The kinds of query that read the most leaf edges themselves, most first, as `name=walked
    /// x average (most)`: a kind that reads many itself goes through every file, however few the
    /// kinds below it read.
    pub(crate) fn direct(&self, count: usize) -> String {
        let mut all: Vec<_> = self.direct.iter().collect();
        all.sort_by_key(|(name, (_, total, most))| (std::cmp::Reverse(*most), std::cmp::Reverse(*total), **name));
        all.into_iter()
            .take(count)
            .map(|(name, (walked, total, most))| {
                let mut kinds: Vec<_> = self
                    .direct_kinds
                    .iter()
                    .filter(|((query, _), _)| query == name)
                    .map(|((_, kind), count)| (*count, *kind))
                    .collect();
                kinds.sort_by_key(|(count, kind)| (std::cmp::Reverse(*count), *kind));
                let kinds = kinds
                    .into_iter()
                    .take(3)
                    .map(|(count, kind)| format!("{kind}:{count}"))
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "{name}={walked}x{} ({most}) [{kinds}]",
                    total / u64::from(*walked).max(1)
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The kinds of query with the widest single query, widest first, as `name=walked x average
    /// (most) [what the widest reached]`: the widest are what everything reading them depends on a
    /// great deal through, and what they reached says whether it is every file's text.
    pub(crate) fn breadth(&self, count: usize) -> String {
        let mut all: Vec<_> = self.breadth.iter().collect();
        all.sort_by_key(|(name, (_, _, most))| (std::cmp::Reverse(*most), **name));
        all.into_iter()
            .take(count)
            .map(|(name, (walked, total, most))| {
                let mix = self.widest_mix.get(name).map(|(_, mix)| mix.as_str()).unwrap_or("");
                format!("{name}={walked}x{} ({most}) [{mix}]", total / u64::from(*walked).max(1))
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// What the flattener holds: the edges numbered, the distinct sets and the words they take, and the
    /// queries whose result is kept. These are what a flattener costs in memory while it is alive, and
    /// what a set of runs in place of words would change.
    pub fn footprint(&self) -> Footprint {
        let (mut runs, mut widest_runs) = (0_u64, 0_u64);
        for set in &self.interned {
            let held = set.runs().len() as u64;
            runs += held;
            widest_runs = widest_runs.max(held);
        }
        Footprint {
            universe: self.edges.len() as u64,
            sets: self.interned.len() as u64,
            runs,
            widest_runs,
            kept: self.done.len() as u64,
        }
    }

    /// What this has done so far.
    pub(crate) fn stats(&self) -> FlattenStats {
        self.stats
    }

    /// How many queries have a result kept.
    pub(crate) fn kept(&self) -> u64 {
        self.done.len() as u64
    }
}

/// The leaf a dependency on `edge` is saved as in place of what the query reads, when `edge` is a
/// query whose value has a saved fingerprint (see `Database::register_cutoff`): the fingerprint of
/// the same argument. `None` when there is no fingerprint to depend on: the query has none, the
/// fingerprint has not been asked for this argument (it has no memo), or the memo being flattened
/// is the fingerprint itself, which has to go through the query it fingerprints.
pub(crate) fn cutoff_edge(
    zalsa: &Zalsa,
    owner: Option<crate::IngredientIndex>,
    edge: QueryEdge,
) -> Option<QueryEdge> {
    let fingerprint = zalsa.cutoff_of(edge.key().ingredient_index())?;
    if owner == Some(fingerprint) {
        return None;
    }
    // SAFETY: Callers prevent database writes throughout the traversal.
    let present = unsafe {
        zalsa
            .lookup_ingredient(fingerprint)
            .as_function()?
            .memo_unchecked(zalsa, edge.key().key_index())
    }
    .is_some();
    present.then(|| {
        QueryEdge::input(crate::key::DatabaseKeyIndex::new(
            fingerprint,
            edge.key().key_index(),
        ))
    })
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
) -> Option<(LeafSet, Closure)> {
    // What this query reaches does not depend on who asks: it is worked out once.
    if let Some((result, closure)) = flattener.done.get(&edge) {
        flattener.stats.reused += 1;
        return Some((Arc::clone(result), *closure));
    }

    // A query whose value has a saved fingerprint is reached as that fingerprint, however it is
    // reached: read by a memo, or through a tracked struct it made.
    if let Some(fingerprint) = cutoff_edge(zalsa, flattener.owner, edge) {
        let mut found = RunBuilder::default();
        found.set(flattener.number(fingerprint));
        let result = flattener.intern(found);
        let closure = Closure::nothing();
        flattener.done.insert(edge, (Arc::clone(&result), closure));
        return Some((result, closure));
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
    };
    let Some(memo) = memo else {
        // A query read that has no memo now: the slot was reused, or the value dropped. What it
        // read cannot be seen, so a memo that reached it is not verified from what is seen.
        flattener.met_no_memo = true;
        return None;
    };

    let order = flattener.next;
    flattener.next += 1;
    flattener.open.insert(edge, order);
    flattener.stack.push(edge);
    flattener.stats.walked += 1;
    let outer = std::mem::replace(&mut flattener.lowlink, usize::MAX);

    let mut found = RunBuilder::default();
    let mut direct = 0u32;
    let mut direct_leaves: Vec<QueryEdge> = Vec::new();
    let mut closure = memo.header().closure();
    for dependency_edge in memo.header().origin().edges() {
        if dependency_edge.is_set() {
            // The members of a restored set, as the edges they are.
            for member in shared_set(dependency_edge.set_number()).edges() {
                let number = flattener.number(member);
                found.set(number);
                direct += 1;
                direct_leaves.push(member);
            }
            continue;
        }
        if dependency_edge.is_watermark() {
            let number = flattener.number(dependency_edge);
            found.set(number);
            direct += 1;
            direct_leaves.push(dependency_edge);
            continue;
        }
        let dependency = zalsa.lookup_ingredient(dependency_edge.key().ingredient_index());

        if dependency.as_function().is_some() {
            if dependency.is_persistable() {
                // A persisted query is serialized itself: depend on it, and do not walk through
                // it (its own dependencies are covered by its own memo).
                let number = flattener.number(dependency_edge);
                found.set(number);
                direct += 1;
                direct_leaves.push(dependency_edge);
            } else if let Some((set, below)) =
                unsafe { leaves_of(zalsa, dependency_edge, flattener) }
            {
                found.or(&set);
                closure.merge(below);
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
            direct += 1;
            direct_leaves.push(leaf);
        }
        flattener.scratch = added;
        closure.merge(std::mem::replace(&mut flattener.carried, Closure::nothing()));
    }

    let result = flattener.intern(found);
    {
        let size = u32::try_from(result.len()).unwrap_or(u32::MAX);
        if flattener.widest_mix.get(ingredient.debug_name()).is_none_or(|(widest, _)| size > *widest) {
            let mut mix: FxHashMap<&'static str, u32> = FxHashMap::default();
            for number in result.numbers() {
                let leaf = flattener.edges[number as usize];
                let kind = if leaf.is_watermark() {
                    "<watermark>"
                } else if leaf.is_set() {
                    "<set>"
                } else {
                    zalsa.lookup_ingredient(leaf.key().ingredient_index()).debug_name()
                };
                *mix.entry(kind).or_default() += 1;
            }
            let mut mix: Vec<_> = mix.into_iter().collect();
            mix.sort_by_key(|(kind, count)| (std::cmp::Reverse(*count), *kind));
            let mix = mix
                .into_iter()
                .take(4)
                .map(|(kind, count)| format!("{kind}:{count}"))
                .collect::<Vec<_>>()
                .join(",");
            flattener.widest_mix.insert(ingredient.debug_name(), (size, mix));
        }
        let (walked, total, most) = flattener.breadth.entry(ingredient.debug_name()).or_default();
        *walked += 1;
        *total += u64::from(size);
        *most = (*most).max(size);
        for leaf in &direct_leaves {
            let kind = if leaf.is_watermark() {
                "<watermark>"
            } else if leaf.is_set() {
                "<set>"
            } else {
                zalsa.lookup_ingredient(leaf.key().ingredient_index()).debug_name()
            };
            *flattener.direct_kinds.entry((ingredient.debug_name(), kind)).or_default() += 1;
        }
        let (walked, total, most) = flattener.direct.entry(ingredient.debug_name()).or_default();
        *walked += 1;
        *total += u64::from(direct);
        *most = (*most).max(direct);
    }
    if flattener.lowlink >= order {
        // The first query of its group: everything the group reaches has been found, and
        // each member reaches exactly what the others do. The group's closure is the members' own
        // together with what they reached (a member's own query is in it too).
        let members: Vec<QueryEdge> = {
            let from = flattener
                .stack
                .iter()
                .rposition(|member| *member == edge)
                .unwrap_or(0);
            flattener.stack[from..].to_vec()
        };
        for member in members.iter().filter(|member| **member != edge) {
            let ingredient = zalsa.lookup_ingredient(member.key().ingredient_index());
            // SAFETY: The caller prevents database writes throughout the traversal.
            let own = ingredient
                .as_function()
                .and_then(|function| unsafe {
                    function.memo_unchecked(zalsa, member.key().key_index())
                })
                .map(|memo| memo.header().closure());
            match own {
                Some(own) => closure.merge(own),
                None => closure.trusted = false,
            }
        }
        while let Some(member) = flattener.stack.pop() {
            flattener.open.remove(&member);
            flattener.done.insert(member, (Arc::clone(&result), closure));
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
    Some((result, closure))
}

thread_local! {
    /// The number the first of the sets the ingredient being deserialized saved was given among
    /// all those restored: the set `n` of the ingredient is the restored set `base + n`.
    static SAVED_BASE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// A restored set: runs of positions in the list of edges its ingredient saved. It is as long as
/// the runs are, not as the set is, so the sets of a crate of a thousand files are a few numbers each.
pub(crate) struct RestoredSet {
    universe: Arc<[QueryEdge]>,
    runs: Box<[(u32, u32)]>,
}

impl RestoredSet {
    /// The edges of the set.
    pub(crate) fn edges(&self) -> impl Iterator<Item = QueryEdge> + '_ {
        self.runs.iter().flat_map(|(first, length)| {
            self.universe[*first as usize..(*first + *length) as usize]
                .iter()
                .copied()
        })
    }
}

/// The sets restored memos refer to, by number: every ingredient's, in the order restored.
///
/// A set is shared by every memo that reaches the same edges (in a crate of a thousand files, the
/// thousand file texts that every function of it depends on through its definition map). A
/// restored memo does not copy the set into an edge list of its own: it holds one edge that
/// refers to the set, so restoring costs a memo what it reads itself, and the set is checked once
/// for all of them (see [`set_changed_after`]).
static SHARED_SETS: std::sync::RwLock<Vec<Arc<RestoredSet>>> = std::sync::RwLock::new(Vec::new());

/// The restored set `number`.
pub(crate) fn shared_set(number: u32) -> Arc<RestoredSet> {
    let sets = SHARED_SETS.read().unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(
        sets.get(number as usize)
            .unwrap_or_else(|| panic!("a memo refers to restored set {number}, which there is not")),
    )
}

/// Hold the sets a saved ingredient listed while its memos are deserialized.
#[cfg(feature = "persistence")]
pub(crate) struct SavedSets;

#[cfg(feature = "persistence")]
impl SavedSets {
    /// The sets of one ingredient: each as runs of positions in `universe`, its list of edges.
    pub(crate) fn install(universe: Vec<QueryEdge>, sets: Vec<Vec<(u32, u32)>>) -> Self {
        let universe: Arc<[QueryEdge]> = Arc::from(universe);
        let mut shared = SHARED_SETS.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        let base = u32::try_from(shared.len()).expect("fewer than 2^32 restored sets");
        shared.extend(sets.into_iter().map(|runs| {
            Arc::new(RestoredSet {
                universe: Arc::clone(&universe),
                runs: runs.into_boxed_slice(),
            })
        }));
        SAVED_BASE.with(|saved| saved.set(base));
        Self
    }
}

/// The edges a restored memo has: one for each saved set it refers to, and its own.
#[cfg(feature = "persistence")]
pub(crate) fn refer_to_saved(sets: &[u32], edges: &[QueryEdge]) -> Box<[QueryEdge]> {
    let base = SAVED_BASE.with(std::cell::Cell::get);
    let all: Box<[QueryEdge]> = sets
        .iter()
        .map(|set| QueryEdge::set(base + set))
        .chain(edges.iter().copied())
        .collect();
    EDGES_EXPANDED.fetch_add(all.len() as u64, std::sync::atomic::Ordering::Relaxed);
    all
}

/// The answers [`set_changed_after`] has given in one revision of one database.
#[derive(Default)]
struct SetAnswers {
    database: usize,
    revision: Option<crate::Revision>,
    /// For each set and revision a memo was verified at: the edge that changed after it, if one did.
    answers: FxHashMap<(u32, crate::Revision), Option<QueryEdge>>,
}

static SET_ANSWERS: std::sync::Mutex<Option<SetAnswers>> = std::sync::Mutex::new(None);

static LAST_BREADTH: std::sync::Mutex<Vec<(&'static str, String)>> = std::sync::Mutex::new(Vec::new());

/// The kinds of query whose sets reached the most edges when `ingredient`'s memos were last
/// flattened for a save, as the flatten report says them (`name=walked x average (most)`): which
/// kinds of query make what reads them depend on a great deal.
pub fn flatten_breadth(ingredient: &str) -> Option<String> {
    LAST_BREADTH
        .lock()
        .ok()?
        .iter()
        .find(|(name, _)| *name == ingredient)
        .map(|(_, breadth)| breadth.clone())
}

pub(crate) fn note_breadth(ingredient: &'static str, breadth: String) {
    if let Ok(mut all) = LAST_BREADTH.lock() {
        all.retain(|(name, _)| *name != ingredient);
        all.push((ingredient, breadth));
    }
}

static SET_CHECKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SET_COMPUTED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SET_MEMBERS_CHECKED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many times a memo's reference to a restored set was verified, how many of those had to
/// check the set's edges (the rest were answered from an earlier memo's check in the same
/// revision), and how many edges those checks looked at: what verifying restored memos costs now
/// that they share their sets.
pub fn set_verifications() -> (u64, u64, u64) {
    (
        SET_CHECKS.load(std::sync::atomic::Ordering::Relaxed),
        SET_COMPUTED.load(std::sync::atomic::Ordering::Relaxed),
        SET_MEMBERS_CHECKED.load(std::sync::atomic::Ordering::Relaxed),
    )
}

/// Whether the leaf `edge` of a set is known not to have changed after `since`, from the input it
/// is, with no query run and no query walked. A leaf that is a persisted query is not known so
/// here: that is for walking what it reads.
pub(crate) fn edge_unchanged(
    db: crate::database::RawDatabase<'_>,
    zalsa: &Zalsa,
    edge: QueryEdge,
    since: crate::Revision,
) -> bool {
    if edge.is_watermark() {
        return zalsa.last_changed_revision(WATERMARK_DURABILITY) <= since;
    }
    if edge.is_set() || edge.kind() != crate::zalsa_local::QueryEdgeKind::Input {
        return false;
    }
    if zalsa
        .lookup_ingredient(edge.key().ingredient_index())
        .as_function()
        .is_some()
    {
        return false;
    }
    matches!(
        edge.key().maybe_changed_after(db, zalsa, since),
        crate::function::VerifyResult::Unchanged { .. }
    )
}

/// The edge of the restored set `number` that changed after `since`, if one did.
///
/// Every memo of a crate refers to the same set, and was verified at the same revision, so the
/// answer is worked out once for all of them and kept until the database changes. Nothing is
/// kept across a change: the answer is a fact about the inputs as they are in one revision.
pub(crate) fn set_changed_after(
    db: crate::database::RawDatabase<'_>,
    zalsa: &Zalsa,
    number: u32,
    since: crate::Revision,
) -> Option<QueryEdge> {
    SET_CHECKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let database = std::ptr::from_ref(zalsa) as usize;
    let revision = zalsa.current_revision();
    {
        let mut kept = SET_ANSWERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let answers = kept.get_or_insert_with(SetAnswers::default);
        if answers.database != database || answers.revision != Some(revision) {
            *answers = SetAnswers {
                database,
                revision: Some(revision),
                answers: FxHashMap::default(),
            };
        } else if let Some(answer) = answers.answers.get(&(number, since)) {
            return *answer;
        }
    }
    SET_COMPUTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut changed = None;
    for edge in shared_set(number).edges() {
        SET_MEMBERS_CHECKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if edge.is_watermark() {
            if zalsa.last_changed_revision(WATERMARK_DURABILITY) > since {
                changed = Some(edge);
                break;
            }
            continue;
        }
        if edge.kind() == crate::zalsa_local::QueryEdgeKind::Input
            && matches!(
                edge.key().maybe_changed_after(db, zalsa, since),
                crate::function::VerifyResult::Changed
            )
        {
            changed = Some(edge);
            break;
        }
    }
    let mut kept = SET_ANSWERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(answers) = kept.as_mut()
        && answers.database == database
        && answers.revision == Some(revision)
    {
        answers.answers.insert((number, since), changed);
    }
    changed
}

/// Whether `edge` is a crate's `data` of the lowest durability. Crates the loader made are written
/// at a higher one (and, whether or not that is under the watermark, are edges of their own or
/// summarized), so these are the ones a tool added.
pub(crate) fn is_low_crate_data(zalsa: &Zalsa, edge: QueryEdge) -> bool {
    if edge.is_summary() {
        return false;
    }
    let ingredient = zalsa.lookup_ingredient(edge.key().ingredient_index());
    ingredient.debug_name() == "data"
        && ingredient.is_lowest_durability(zalsa, edge.key().key_index())
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
            if edge.is_summary() || !seen.insert(edge) {
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
