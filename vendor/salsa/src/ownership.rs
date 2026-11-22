//! Opt-in semantic ownership of cached query payloads.
//!
//! Resolvers run inside the query frame; retirement uses only native reverse links.
//! A link identifies the exact function and full generational input ID. A publication
//! token prevents deferred destruction of a replaced memo from removing its successor.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use crate::database::RawDatabase;
use crate::hash::FxHashMap;
use crate::ingredient::Jar;
use crate::zalsa::Zalsa;
use crate::{Database, DatabaseKeyIndex, Id, IngredientIndex};

/// Semantic lifetimes represented by the arguments of an opted-in query.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
pub enum QueryOwnership {
    /// Arguments contain no context-local identities.
    Shared,
    /// All these roots must finish before the payload can retire.
    Owned(Box<[Id]>),
    /// Ownership is not established; preserve the payload.
    Unclassified,
}

impl QueryOwnership {
    /// Deduplicate roots; an empty set describes shared arguments.
    pub fn owned(roots: impl IntoIterator<Item = Id>) -> Self {
        let mut roots: Vec<_> = roots.into_iter().collect();
        roots.sort_unstable_by_key(|id| id.as_bits());
        roots.dedup();
        if roots.is_empty() {
            Self::Shared
        } else {
            Self::Owned(roots.into_boxed_slice())
        }
    }

    fn roots(&self) -> &[Id] {
        match self {
            Self::Owned(roots) => roots,
            _ => &[],
        }
    }
}

type Resolver = Box<dyn Fn(RawDatabase<'_>, &Zalsa, Id) -> QueryOwnership + Send + Sync>;

/// Register an argument resolver against a generated tracked-function jar type.
///
/// Register once, before this query executes (also after restoring a database).
/// `View` must be a registered database view, normally the query's database trait.
/// Registration is a cache policy change, not a semantic input edit.
/// Resolvers may read argument and location records, but must not run analysis.
pub fn register_query_ownership<Query: Jar, View: Database + ?Sized>(
    db: &mut impl Database,
    resolver: fn(&View, Id) -> QueryOwnership,
) {
    let zalsa = db.zalsa_mut();
    let ingredient = zalsa.lookup_jar_by_type::<Query>();
    assert!(
        zalsa.lookup_ingredient(ingredient).as_function().is_some(),
        "ownership requires a tracked-function jar"
    );
    assert!(
        !zalsa.ownership_resolvers.contains_key(&ingredient),
        "ownership resolver already registered"
    );
    zalsa.ownership_resolvers.insert(
        ingredient,
        Box::new(move |raw, zalsa, id| {
            let caster = zalsa.views().downcaster_for::<View>();
            // SAFETY: The raw database and downcaster belong to this same Zalsa storage.
            resolver(unsafe { caster.downcast_unchecked(raw) }, id)
        }),
    );
}

/// Structural index accounting, independent of allocator measurements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwnerIndexStats {
    pub roots: usize,
    pub live_roots: usize,
    pub buckets: usize,
    pub memberships: usize,
    pub capacity: usize,
    /// Current RAM payloads explicitly classified as shared.
    pub shared: usize,
    /// Current RAM payloads whose ownership could not be established.
    pub unclassified: usize,
}

#[derive(Default)]
struct Root {
    retired: bool,
    entries: FxHashMap<DatabaseKeyIndex, u64>,
}

#[derive(Default)]
struct IndexState {
    roots: FxHashMap<Id, Root>,
    next_publication: u64,
    shared: usize,
    unclassified: usize,
}

#[derive(Default)]
pub(crate) struct OwnerIndex(Mutex<IndexState>);

impl OwnerIndex {
    pub(crate) fn activate(&self, roots: &[Id]) {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        for &root in roots {
            state.roots.entry(root).or_default().retired = false;
        }
    }

    pub(crate) fn contains(&self, root: Id) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .roots
            .contains_key(&root)
    }

    pub(crate) fn stats(&self) -> OwnerIndexStats {
        let state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        OwnerIndexStats {
            shared: state.shared,
            unclassified: state.unclassified,
            roots: state.roots.len(),
            live_roots: state.roots.values().filter(|root| !root.retired).count(),
            buckets: state
                .roots
                .values()
                .filter(|root| !root.entries.is_empty())
                .count(),
            memberships: state.roots.values().map(|root| root.entries.len()).sum(),
            capacity: state
                .roots
                .values()
                .map(|root| root.entries.capacity())
                .sum(),
        }
    }

    fn attach(self: &Arc<Self>, key: DatabaseKeyIndex, declaration: &QueryOwnership) -> Membership {
        let roots = declaration.roots();
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        state.next_publication = state
            .next_publication
            .checked_add(1)
            .expect("ownership publication token exhausted");
        let token = state.next_publication;
        match declaration {
            QueryOwnership::Shared => state.shared += 1,
            QueryOwnership::Unclassified => state.unclassified += 1,
            _ => {}
        }
        for &root in roots {
            state
                .roots
                .entry(root)
                .or_default()
                .entries
                .insert(key, token);
        }
        Membership {
            index: Arc::downgrade(self),
            key,
            token,
        }
    }

    fn remove(&self, membership: &Membership, declaration: &QueryOwnership) {
        let roots = declaration.roots();
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        match declaration {
            QueryOwnership::Shared => state.shared -= 1,
            QueryOwnership::Unclassified => state.unclassified -= 1,
            _ => {}
        }
        for root in roots {
            let Some(root) = state.roots.get_mut(root) else {
                continue;
            };
            if root.entries.get(&membership.key) == Some(&membership.token) {
                root.entries.remove(&membership.key);
                if root.entries.is_empty() {
                    root.entries = FxHashMap::default();
                }
            }
        }
    }
}

struct Membership {
    index: Weak<OwnerIndex>,
    key: DatabaseKeyIndex,
    token: u64,
}

/// Canonical declaration plus ephemeral reverse-link handle, only for opted-in memos.
#[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct MemoOwnership {
    declaration: QueryOwnership,
    #[cfg_attr(feature = "persistence", serde(skip))]
    membership: Mutex<Option<Membership>>,
    #[cfg_attr(feature = "persistence", serde(skip))]
    superseded: AtomicBool,
}

impl std::fmt::Debug for MemoOwnership {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.declaration.fmt(f)
    }
}

impl MemoOwnership {
    pub(crate) fn new(declaration: QueryOwnership) -> Box<Self> {
        let declaration = match declaration {
            QueryOwnership::Owned(roots) => QueryOwnership::owned(roots),
            other => other,
        };
        Box::new(Self {
            declaration,
            membership: Mutex::new(None),
            superseded: AtomicBool::new(false),
        })
    }

    pub(crate) fn attach(&self, index: &Arc<OwnerIndex>, key: DatabaseKeyIndex) {
        let mut membership = self.membership.lock().unwrap_or_else(|p| p.into_inner());
        if membership.is_none() && !self.superseded.load(Ordering::Relaxed) {
            *membership = Some(index.attach(key, &self.declaration));
        }
    }

    pub(crate) fn supersede(&self) {
        self.superseded.store(true, Ordering::Relaxed);
        self.detach();
    }

    #[cfg(feature = "salsa_unstable")]
    pub(crate) fn allocation_size(&self) -> usize {
        std::mem::size_of::<Self>() + std::mem::size_of_val(self.declaration.roots())
    }

    pub(crate) fn copy_declaration(&self) -> Box<Self> {
        Self::new(self.declaration.clone())
    }

    pub(crate) fn eligible(&self, index: &OwnerIndex, report: &mut OwnerRetirementReport) -> bool {
        let state = index.0.lock().unwrap_or_else(|p| p.into_inner());
        for root in self.declaration.roots() {
            report.owner_checks += 1;
            if !state.roots.get(root).is_some_and(|root| root.retired) {
                return false;
            }
        }
        !self.declaration.roots().is_empty()
    }

    pub(crate) fn detach(&self) {
        let membership = self
            .membership
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        if let Some(membership) = membership {
            if let Some(index) = membership.index.upgrade() {
                index.remove(&membership, &self.declaration);
            }
        }
    }
}

impl Drop for MemoOwnership {
    fn drop(&mut self) {
        self.detach();
    }
}

impl Zalsa {
    pub(crate) fn resolve_ownership(
        &self,
        db: RawDatabase<'_>,
        key: DatabaseKeyIndex,
    ) -> Option<Box<MemoOwnership>> {
        let resolver = self.ownership_resolvers.get(&key.ingredient_index())?;
        let declaration = resolver(db, self, key.key_index());
        let declaration = if declaration
            .roots()
            .iter()
            .all(|root| self.owner_index.contains(*root))
        {
            declaration
        } else {
            // An undeclared lifetime must never be silently treated as shared or complete.
            QueryOwnership::Unclassified
        };
        Some(MemoOwnership::new(declaration))
    }
}

/// Register or reactivate roots before any of their queries execute.
/// Does not change the semantic revision or run LRU eviction.
pub fn activate_owner_roots(db: &mut impl Database, roots: &[Id]) {
    db.zalsa_mut().owner_index.activate(roots);
}

/// Whether an argument identity is a registered context root (live or completed).
pub fn is_owner_root(db: &dyn Database, root: Id) -> bool {
    db.zalsa().owner_index.contains(root)
}

/// Read structural accounting without invoking queries or changing memo values.
pub fn owner_index_stats(db: &mut impl Database) -> OwnerIndexStats {
    db.zalsa_mut_for_inspection().owner_index.stats()
}

pub(crate) type Resolvers = FxHashMap<IngredientIndex, Resolver>;

/// Work and release counts for one exclusive owner-retirement boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnerRetirementReport {
    /// Reverse links visited, including links shared by several completed roots.
    pub visited: usize,
    /// Distinct exact function/key pairs selected.
    pub selected: usize,
    /// Canonical owner states checked, bounded by selected declarations.
    pub owner_checks: usize,
    /// Selected payloads protected by an unfinished owner.
    pub protected: usize,
    /// Current payloads completely released from RAM.
    pub released: usize,
    /// Eligible payloads retained, e.g. missing spill store or non-recomputable origin.
    pub retained: usize,
    /// Eligible memos whose origin or provisional state forbids release.
    pub refused: usize,
    /// Persisted payloads retained because no spill store is configured.
    pub spill_unavailable: usize,
    /// Persisted payloads retained after an unsuccessful encoding.
    pub spill_failed: usize,
    /// All current opted-in payloads protected by an incomplete classifier.
    pub unclassified: usize,
    /// Superseded memo allocations destroyed before current-memo selection.
    pub deferred_released: usize,
    /// Spill activity attributable to these exact memos.
    pub spill: crate::SpillReport,
}

/// Complete roots and release their eligible current payloads using only reverse links.
///
/// Holds exclusive database access without changing its semantic revision or running LRU.
/// References to previous values cannot survive this boundary. Re-entry must explicitly
/// reactivate roots before fetching. Unfinished owners always protect shared memberships.
pub fn retire_owned_values(db: &mut impl Database, roots: &[Id]) -> OwnerRetirementReport {
    let zalsa = db.zalsa_mut();
    let mut report = OwnerRetirementReport::default();
    report.deferred_released = zalsa.drain_deferred_memos();
    let keys = {
        let mut state = zalsa
            .owner_index
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let mut keys = crate::hash::FxHashSet::default();
        for root in roots.iter().copied().collect::<crate::hash::FxHashSet<_>>() {
            if let Some(root) = state.roots.get_mut(&root) {
                root.retired = true;
                report.visited += root.entries.len();
                keys.extend(root.entries.keys().copied());
            }
        }
        keys
    };
    report.selected = keys.len();
    let store = crate::spill::spill_store();
    let counters = crate::spill::SpillCounters::default();
    for key in keys {
        // SAFETY: zalsa_mut cancelled readers and grants exclusive storage access.
        unsafe {
            zalsa
                .lookup_ingredient(key.ingredient_index())
                .retire_owned_value(
                    zalsa,
                    key.key_index(),
                    store.as_deref(),
                    &counters,
                    &mut report,
                );
        }
    }
    if let Some(store) = store {
        store.flush();
    }
    report.unclassified = zalsa
        .owner_index
        .0
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .unclassified;
    report.spill = counters.report();
    report
}
