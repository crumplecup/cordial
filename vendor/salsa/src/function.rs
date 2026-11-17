pub(crate) use maybe_changed_after::VerifyResult;
pub(crate) use memo::ErasedMemo;
pub(crate) use sync::{ClaimGuard, ClaimResult, Reentrancy, SyncGuard, SyncOwner, SyncTable};

use std::any::Any;
use std::fmt;
use std::ptr::NonNull;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use crate::cycle::{CycleRecoveryStrategy, IterationStamp, ProvisionalStatus};
use crate::database::RawDatabase;
use crate::flatten::EdgeFlattener;
use crate::function::delete::DeletedEntries;
use crate::hash::{FxHashSet, FxIndexSet};
use crate::ingredient::Ingredient;
use crate::key::DatabaseKeyIndex;
use crate::plumbing::{self, MemoIngredientMap};
use crate::salsa_struct::SalsaStructInDb;
use crate::sync::Arc;
use crate::table::Table;
use crate::table::memo::{MemoSlot, MemoTableTypes};
use crate::views::DatabaseDownCaster;
use crate::zalsa::{IngredientIndex, JarKind, MemoIngredientIndex, Zalsa};
use crate::zalsa_local::QueryEdge;
use crate::{Cycle, Id, Revision};

#[cfg(feature = "accumulator")]
mod accumulated;
mod backdate;
mod delete;
mod diff_outputs;
mod eviction;
mod execute;
mod fetch;
mod maybe_changed_after;
mod memo;
mod specify;
mod sync;

pub use eviction::{EvictionPolicy, HasCapacity, Lru, NoopEviction, cap_capacities};

pub type Memo<C> = memo::Memo<C>;

/// Configuration for a Salsa function ingredient.
///
/// # Safety
///
/// For every lifetime `'db`, `Output<'db>` must be safe for Salsa to retain
/// after erasing `'db` and to use after rebranding it with a later database
/// lifetime. This is guaranteed when the output implements [`crate::SalsaValue`]
/// or when it is the same `'static` type for every `'db`.
pub unsafe trait Configuration: Any {
    const DEBUG_NAME: &'static str;
    const LOCATION: crate::ingredient::Location;
    const PERSIST: bool;

    /// The database that this function is associated with.
    type DbView: ?Sized + crate::Database;

    /// The "salsa struct type" that this function is associated with.
    /// This can be just `salsa::Id` for functions that intern their arguments
    /// and are not clearly associated with any one salsa struct.
    type SalsaStruct<'db>: SalsaStructInDb;

    /// The input to the function
    type Input<'db>: Send + Sync;

    /// The value computed by the function.
    type Output<'db>: Send + Sync;

    /// The eviction policy for this function's memoized values.
    type Eviction: EvictionPolicy;

    /// Determines whether this function can recover from being a participant in a cycle
    /// (and, if so, how).
    const CYCLE_STRATEGY: CycleRecoveryStrategy;

    /// Invokes after a new result `new_value` has been computed for which an older memoized value
    /// existed `old_value`, or in fixpoint iteration. Returns true if the new value is equal to
    /// the older one.
    ///
    /// This invokes user code in form of the `Eq` impl.
    fn values_equal<'db>(old_value: &Self::Output<'db>, new_value: &Self::Output<'db>) -> bool;

    /// Convert from the id used internally to the value that execute is expecting.
    /// This is a no-op if the input to the function is a salsa struct.
    fn id_to_input(zalsa: &Zalsa, key: Id) -> Self::Input<'_>;

    /// Returns the size of any heap allocations in the output value, in bytes.
    fn heap_size(_value: &Self::Output<'_>) -> Option<usize> {
        None
    }

    /// Invoked when we need to compute the value for the given key, either because we've never
    /// computed it before or because the old one relied on inputs that have changed.
    ///
    /// This invokes the function the user wrote.
    fn execute<'db>(db: &'db Self::DbView, input: Self::Input<'db>) -> Self::Output<'db>;

    /// Get the cycle recovery initial value.
    fn cycle_initial<'db>(
        db: &'db Self::DbView,
        id: Id,
        input: Self::Input<'db>,
    ) -> Self::Output<'db>;

    /// Decide what value to use for this cycle iteration. Takes ownership of the new value
    /// and returns an owned value to use.
    ///
    /// The function is called for every iteration of the cycle head, regardless of whether the cycle
    /// has converged (the values are equal).
    ///
    /// # Id
    ///
    /// The id can be used to uniquely identify the query instance. This can be helpful
    /// if the cycle function has to re-identify a value it returned previously.
    ///
    /// # Values
    ///
    /// The `last_provisional_value` is the value from the previous iteration of this cycle
    /// and `value` is the new value that was computed in the current iteration.
    ///
    /// # Iteration count
    ///
    /// The `iteration` parameter isn't guaranteed to start from zero or to be contiguous:
    ///
    /// * **Initial value**: `iteration` may be non-zero on the first call for a given query if that
    ///   query becomes the outermost cycle head after a nested cycle complete a few iterations. In this case,
    ///   `iteration` continues from the nested cycle's iteration count rather than resetting to zero.
    /// * **Non-contiguous values**: The iteration count can be non-contigious for cycle heads
    ///   that are only conditionally part of a cycle.
    ///
    /// # Return value
    ///
    /// The function should return the value to use for this iteration. This can be the `value`
    /// that was computed, or a different value (e.g., a fallback value). This cycle will continue
    /// iterating until the returned value equals the previous iteration's value.
    fn recover_from_cycle<'db>(
        db: &'db Self::DbView,
        cycle: &Cycle,
        last_provisional_value: &Self::Output<'db>,
        value: Self::Output<'db>,
        input: Self::Input<'db>,
    ) -> Self::Output<'db>;

    /// Serialize the output type using `serde`.
    ///
    /// Panics if the value is not persistable, i.e. `Configuration::PERSIST` is `false`.
    fn serialize<S>(value: &Self::Output<'_>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: plumbing::serde::Serializer;

    /// Deserialize the output type using `serde`.
    ///
    /// Panics if the value is not persistable, i.e. `Configuration::PERSIST` is `false`.
    fn deserialize<'de, D>(deserializer: D) -> Result<Self::Output<'static>, D::Error>
    where
        D: plumbing::serde::Deserializer<'de>;
}

/// The function-specific interface for an [`Ingredient`].
///
/// This type is public because it appears in the public [`Ingredient`] trait, but it can only be
/// constructed and used inside salsa.
#[doc(hidden)]
pub struct FunctionIngredientRef<'db> {
    ingredient: &'db dyn FunctionIngredient,
}

impl<'db> FunctionIngredientRef<'db> {
    fn new(ingredient: &'db dyn FunctionIngredient) -> Self {
        Self { ingredient }
    }

    pub(crate) fn memo(&self, zalsa: &'db Zalsa, input: Id) -> Option<ErasedMemo<'db>> {
        self.ingredient.memo(zalsa, input)
    }

    pub(crate) fn sync_table(&self) -> &'db SyncTable {
        self.ingredient.sync_table()
    }

    /// Returns the cached memo for `input`, relying on the caller to prevent its storage from
    /// being reused or freed (unlike [`Self::memo`], which asserts that an interned key was
    /// interned in the latest revision, and so fails for the memos of keys that have been idle).
    ///
    /// # Safety
    ///
    /// The caller must prevent database writes for `'db`, including query execution that can
    /// reuse slots or modify memos. Concurrent read-only access is allowed.
    pub(crate) unsafe fn memo_unchecked(
        &self,
        zalsa: &'db Zalsa,
        input: Id,
    ) -> Option<ErasedMemo<'db>> {
        // SAFETY: The caller prevents database writes.
        unsafe { self.ingredient.memo_unchecked(zalsa, input) }
    }

    /// Returns information about the current provisional status of `input`.
    ///
    /// Is it a provisional value, a poisoned provisional memo, or has it been finalized and in
    /// which iteration.
    ///
    /// Returns `None` if `input` doesn't exist.
    pub(crate) fn provisional_status(
        &self,
        zalsa: &'db Zalsa,
        input: Id,
    ) -> Option<ProvisionalStatus<'db>> {
        self.ingredient.provisional_status(zalsa, input)
    }
}

pub(crate) trait FunctionIngredient: Send + Sync {
    fn memo<'db>(&'db self, zalsa: &'db Zalsa, input: Id) -> Option<ErasedMemo<'db>>;

    /// # Safety
    ///
    /// The caller must prevent database writes for `'db`, including query execution that can
    /// reuse slots or modify memos. Concurrent read-only access is allowed.
    unsafe fn memo_unchecked<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        input: Id,
    ) -> Option<ErasedMemo<'db>>;

    fn sync_table(&self) -> &SyncTable;

    /// Returns information about the current provisional status of `input`.
    ///
    /// Is it a provisional value, a poisoned provisional memo, or has it been finalized and in
    /// which iteration.
    ///
    /// Returns `None` if `input` doesn't exist.
    fn provisional_status<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        input: Id,
    ) -> Option<ProvisionalStatus<'db>>;
}

/// Function ingredients are the "workhorse" of salsa.
///
/// They are used for tracked functions, for the "value" fields of tracked structs, and for the fields of input structs.
/// The function ingredient is fairly complex and so its code is spread across multiple modules, typically one per method.
/// The main entry points are:
///
/// * the `fetch` method, which is invoked when the function is called by the user's code;
///   it will return a memoized value if one exists, or execute the function otherwise.
/// * the `specify` method, which can only be used when the key is an entity created by the active query.
///   It sets the value of the function imperatively, so that when later fetches occur, they'll return this value.
/// * the `store` method, which can only be invoked with an `&mut` reference, and is to set input fields.
pub struct IngredientImpl<C: Configuration> {
    /// The ingredient index we were assigned in the database.
    /// Used to construct `DatabaseKeyIndex` values.
    index: IngredientIndex,

    /// The index for the memo/sync tables
    ///
    /// This may be a [`crate::memo_ingredient_indices::MemoIngredientSingletonIndex`] or a
    /// [`crate::memo_ingredient_indices::MemoIngredientIndices`], depending on whether the
    /// tracked function's struct is a plain salsa struct or an enum `#[derive(Supertype)]`.
    memo_ingredient_indices: <C::SalsaStruct<'static> as SalsaStructInDb>::MemoIngredientMap,

    /// Eviction policy - type determined by Configuration.
    /// Used to find memos to throw out when we have too many memoized values.
    eviction: C::Eviction,

    /// An downcaster to `C::DbView`.
    ///
    /// # Safety
    ///
    /// The supplied database must be be the same as the database used to construct the [`Views`]
    /// instances that this downcaster was derived from.
    view_caster: OnceLock<DatabaseDownCaster<C::DbView>>,

    sync_table: SyncTable,

    /// When `fetch` and friends executes, they return a reference to the
    /// value stored in the memo that is extended to live as long as the `&self`
    /// reference we start with. This means that whenever we remove something
    /// from `memo_map` with an `&self` reference, there *could* be references to its
    /// internals still in use. Therefore we push the memo into this queue and
    /// only *actually* free up memory when a new revision starts (which means
    /// we have an `&mut` reference to self).
    ///
    /// You might think that we could do this only if the memo was verified in the
    /// current revision: you would be right, but we are being defensive, because
    /// we don't know that we can trust the database to give us the same runtime
    /// everytime and so forth.
    deleted_entries: DeletedEntries<C>,
}

impl<C> IngredientImpl<C>
where
    C: Configuration,
{
    pub fn new(
        index: IngredientIndex,
        memo_ingredient_indices: <C::SalsaStruct<'static> as SalsaStructInDb>::MemoIngredientMap,
        eviction_capacity: usize,
    ) -> Self {
        Self {
            index,
            memo_ingredient_indices,
            eviction: C::Eviction::new(eviction_capacity),
            deleted_entries: Default::default(),
            view_caster: OnceLock::new(),
            sync_table: SyncTable::new(index),
        }
    }

    /// Set the view-caster for this tracked function ingredient, if it has
    /// not already been initialized.
    #[inline]
    pub fn get_or_init(
        &self,
        view_caster: impl FnOnce() -> DatabaseDownCaster<C::DbView>,
    ) -> &Self {
        // Note that we must set this lazily as we don't have access to the database
        // type when ingredients are registered into the `Zalsa`.
        self.view_caster.get_or_init(view_caster);
        self
    }

    #[inline]
    pub fn database_key_index(&self, key: Id) -> DatabaseKeyIndex {
        DatabaseKeyIndex::new(self.index, key)
    }

    /// Set eviction capacity. Only available when eviction policy supports it.
    pub fn set_capacity(&mut self, capacity: usize)
    where
        C::Eviction: HasCapacity,
    {
        self.eviction.set_capacity(capacity);
    }

    /// Returns a reference to the memo value that lives as long as self.
    /// This is UNSAFE: the caller is responsible for ensuring that the
    /// memo will not be released so long as the `&self` is valid.
    /// This is done by (a) ensuring the memo is present in the memo-map
    /// when this function is called and (b) ensuring that any entries
    /// removed from the memo-map are added to `deleted_entries`, which is
    /// only cleared with `&mut self`.
    unsafe fn extend_memo_lifetime<'this>(
        &'this self,
        memo: &memo::Memo<C>,
    ) -> &'this memo::Memo<C> {
        // SAFETY: the caller must guarantee that the memo will not be released before `&self`
        unsafe { std::mem::transmute(memo) }
    }

    fn insert_memo<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        id: Id,
        mut memo: memo::Memo<C>,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> &'db memo::Memo<C> {
        if let Some(tracked_struct_ids) = memo.header.revisions.tracked_struct_ids_mut() {
            tracked_struct_ids.shrink_to_fit();
        }

        // We convert to a `NonNull` here as soon as possible because we are going to alias
        // into the `Box`, which is a `noalias` type.
        // FIXME: Use `Box::into_non_null` once stable
        let memo = NonNull::from(Box::leak(Box::new(memo)));

        if let Some(old_value) =
            self.insert_memo_into_table_for(zalsa, id, memo, memo_ingredient_index)
        {
            // In case there is a reference to the old memo out there, we have to store it
            // in the deleted entries. This will get cleared when a new revision starts.
            //
            // SAFETY: Once the revision starts, there will be no outstanding borrows to the
            // memo contents, and so it will be safe to free.
            unsafe { self.deleted_entries.push(old_value) };
        }
        // SAFETY: memo has been inserted into the table
        unsafe { self.extend_memo_lifetime(memo.as_ref()) }
    }

    #[inline]
    fn memo_ingredient_index(&self, zalsa: &Zalsa, id: Id) -> MemoIngredientIndex {
        self.memo_ingredient_indices.get_zalsa_id(zalsa, id)
    }

    fn view_caster(&self) -> &DatabaseDownCaster<C::DbView> {
        self.view_caster
            .get()
            .expect("tracked function ingredients cannot be accessed before calling `init`")
    }
}

impl<C> FunctionIngredient for IngredientImpl<C>
where
    C: Configuration,
{
    fn memo<'db>(&'db self, zalsa: &'db Zalsa, input: Id) -> Option<ErasedMemo<'db>> {
        // SAFETY: `self` is borrowed for `'db`, so `deleted_entries` retains every allocation
        // observed in this slot for that lifetime.
        let memo_slot = unsafe {
            MemoSlot::new(
                zalsa.memo_table_for::<C::SalsaStruct<'_>>(input),
                self.memo_ingredient_index(zalsa, input),
            )
        };

        memo_slot.get_erased()
    }

    unsafe fn memo_unchecked<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        input: Id,
    ) -> Option<ErasedMemo<'db>> {
        // SAFETY: The caller prevents database writes, keeping the slot and memo alive for `'db`.
        let memo_slot = unsafe {
            MemoSlot::new(
                zalsa.table().dyn_memos_unchecked(input),
                self.memo_ingredient_index(zalsa, input),
            )
        };

        memo_slot.get_erased()
    }

    fn sync_table(&self) -> &SyncTable {
        &self.sync_table
    }

    /// Returns `final` if the memo has the `verified_final` flag set.
    ///
    /// Otherwise, the value is still provisional or the provisional memo has been poisoned. It
    /// also returns the iteration in which this memo was created (always 0 except for cycle
    /// heads).
    fn provisional_status<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        input: Id,
    ) -> Option<ProvisionalStatus<'db>> {
        let memo =
            self.get_memo_from_table_for(zalsa, input, self.memo_ingredient_index(zalsa, input))?;

        if memo.value.is_none() && memo.header.may_be_provisional() {
            return Some(ProvisionalStatus::Poisoned {
                iteration: memo.header.revisions.iteration(),
                verified_at: memo.header.verified_at.load(),
            });
        }

        Some(memo.header.provisional_status())
    }
}

impl<C> Ingredient for IngredientImpl<C>
where
    C: Configuration,
{
    fn location(&self) -> &'static crate::ingredient::Location {
        &C::LOCATION
    }

    fn ingredient_index(&self) -> IngredientIndex {
        self.index
    }

    unsafe fn maybe_changed_after(
        &self,
        _zalsa: &Zalsa,
        db: RawDatabase<'_>,
        input: Id,
        revision: Revision,
    ) -> VerifyResult {
        // SAFETY: The `db` belongs to the ingredient as per caller invariant
        let db = unsafe { self.view_caster().downcast_unchecked(db) };
        self.maybe_changed_after(db, input, revision)
    }

    unsafe fn collect_minimum_serialized_edges(
        &self,
        zalsa: &Zalsa,
        edge: QueryEdge,
        serialized_edges: &mut FxIndexSet<QueryEdge>,
        flattener: &mut EdgeFlattener,
    ) {
        // SAFETY: The caller prevents database writes throughout the traversal.
        if let Some((set, closure)) = unsafe { crate::flatten::leaves_of(zalsa, edge, flattener) } {
            serialized_edges.extend(flattener.edges_of(&set));
            flattener.carried.merge(closure);
        }
    }

    fn as_function(&self) -> Option<FunctionIngredientRef<'_>> {
        Some(FunctionIngredientRef::new(self))
    }

    fn flatten_cycle_head_dependencies(
        &self,
        zalsa: &Zalsa,
        id: Id,
        flattened_input_outputs: &mut FxIndexSet<QueryEdge>,
        seen: &mut FxHashSet<DatabaseKeyIndex>,
    ) {
        flatten_cycle_head_dependencies(
            self,
            zalsa,
            self.database_key_index(id),
            C::CYCLE_STRATEGY,
            flattened_input_outputs,
            seen,
        );
    }

    fn mark_validated_output(
        &self,
        zalsa: &Zalsa,
        executor: DatabaseKeyIndex,
        output_key: crate::Id,
    ) {
        let memo_ingredient_index = self.memo_ingredient_index(zalsa, output_key);
        let database_key_index = self.database_key_index(output_key);
        let memo_slot = self.memo_slot(zalsa, output_key, memo_ingredient_index);
        specify::validate_specified_value(zalsa, executor, database_key_index, memo_slot);
    }

    fn remove_stale_output(
        &self,
        _zalsa: &Zalsa,
        _executor: DatabaseKeyIndex,
        _stale_output_key: crate::Id,
    ) {
        // This function is invoked when a query Q specifies the value for `stale_output_key` in rev 1,
        // but not in rev 2. We don't do anything in this case, we just leave the (now stale) memo.
        // Since its `verified_at` field has not changed, it will be considered dirty if it is invoked.
    }

    fn requires_reset_for_new_revision(&self) -> bool {
        true
    }

    fn reset_for_new_revision(&mut self, table: &mut Table) {
        let (mut nominated, mut dropped, mut no_value, mut refused) = (0_u64, 0_u64, 0_u64, 0_u64);
        let held = crate::allocated_net_bytes();
        self.eviction.for_each_evicted(|evict| {
            let ingredient_index = table.ingredient_index(evict);
            nominated += 1;
            match Self::evict_value_from_memo_for(
                table.memos_mut(evict),
                self.memo_ingredient_indices.get(ingredient_index),
            ) {
                memo::Eviction::Dropped => dropped += 1,
                memo::Eviction::NoValue => no_value += 1,
                memo::Eviction::Refused => refused += 1,
            }
        });
        if nominated > 0 {
            ::tracing::debug!(
                target: "salsa::eviction",
                ingredient = crate::eviction_label(C::DEBUG_NAME),
                nominated,
                dropped,
                no_value,
                refused,
                freed = held
                    .zip(crate::allocated_net_bytes())
                    .map_or(0, |(held, now)| u64::try_from(held - now).unwrap_or(0)),
                "values evicted"
            );
        }

        self.deleted_entries.clear();
    }

    fn debug_name(&self) -> &'static str {
        C::DEBUG_NAME
    }

    fn jar_kind(&self) -> JarKind {
        JarKind::TrackedFn
    }

    fn memo_table_types(&self) -> &Arc<MemoTableTypes> {
        unreachable!("function does not allocate pages")
    }

    fn memo_table_types_mut(&mut self) -> &mut Arc<MemoTableTypes> {
        unreachable!("function does not allocate pages")
    }

    #[cfg(feature = "accumulator")]
    unsafe fn accumulated<'db>(
        &'db self,
        db: RawDatabase<'db>,
        key_index: Id,
    ) -> (
        Option<&'db crate::accumulator::accumulated_map::AccumulatedMap>,
        crate::accumulator::accumulated_map::InputAccumulatedValues,
    ) {
        // SAFETY: The `db` belongs to the ingredient as per caller invariant
        let db = unsafe { self.view_caster().downcast_unchecked(db) };
        self.accumulated_map(db, key_index)
    }

    fn is_persistable(&self) -> bool {
        C::PERSIST
    }

    #[cfg(feature = "persistence")]
    fn init_view_caster(&self, zalsa: &Zalsa) {
        if let Some(caster) = zalsa.views().try_downcaster_for::<C::DbView>() {
            self.view_caster.get_or_init(|| *caster);
        }
    }

    unsafe fn verify_persisted_by_closure(&self, zalsa: &Zalsa, db: RawDatabase<'_>) {
        // SAFETY: The caller keeps queries from running, and `db` is this ingredient's database.
        unsafe { persistence::verify_by_closure(self, zalsa, db) };
    }

    unsafe fn verify_persisted(
        &self,
        zalsa: &Zalsa,
        db: RawDatabase<'_>,
        part: usize,
        parts: usize,
    ) {
        if !C::PERSIST {
            return;
        }
        let started = std::time::Instant::now();
        let (mut memos, mut current, mut unchanged, mut changed) = (0u64, 0u64, 0u64, 0u64);
        let mut seen = 0usize;
        for entry in <C::SalsaStruct<'_> as SalsaStructInDb>::entries(zalsa) {
            let memo_ingredient_index = self.memo_ingredient_indices.get(entry.ingredient_index());
            let Some(memo) =
                self.get_memo_from_table_for(zalsa, entry.key_index(), memo_ingredient_index)
            else {
                continue;
            };
            let verified_at = memo.header.verified_at.load();
            if !memo.should_serialize() {
                continue;
            }
            // This part's memos are every `parts`th one, in the order the ingredient lists them.
            seen += 1;
            if (seen - 1) % parts.max(1) != part {
                continue;
            }
            memos += 1;
            if verified_at == zalsa.current_revision() {
                current += 1;
                continue;
            }
            // SAFETY: The caller guarantees `db` is this ingredient's database.
            let db = unsafe { self.view_caster().downcast_unchecked(db) };
            // Verifying marks the memo verified now if nothing it depends on changed; if
            // something did, the query runs again and its memo is replaced.
            match self.maybe_changed_after(db, entry.key_index(), verified_at) {
                VerifyResult::Changed => changed += 1,
                VerifyResult::Unchanged { .. } => unchanged += 1,
            }
        }
        if memos > 0 {
            ::tracing::debug!(
                target: "salsa::persistence",
                ingredient = C::DEBUG_NAME,
                part,
                parts,
                memos,
                already_current = current,
                unchanged_after_walking = unchanged,
                changed_and_run_again = changed,
                micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
                "verified the persisted memos before they are saved"
            );
        }
    }

    fn should_serialize(&self, zalsa: &Zalsa) -> bool {
        if !C::PERSIST {
            return false;
        }

        // We only serialize the query if there are any memos associated with it.
        for entry in <C::SalsaStruct<'_> as SalsaStructInDb>::entries(zalsa) {
            let memo_ingredient_index = self.memo_ingredient_indices.get(entry.ingredient_index());

            let memo =
                self.get_memo_from_table_for(zalsa, entry.key_index(), memo_ingredient_index);

            if memo.is_some_and(|memo| memo.should_serialize()) {
                return true;
            }
        }

        false
    }

    #[cfg(feature = "persistence")]
    unsafe fn serialize<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        f: &mut dyn FnMut(&dyn erased_serde::Serialize),
    ) {
        f(&persistence::SerializeIngredient {
            zalsa,
            ingredient: self,
        })
    }

    #[cfg(feature = "persistence")]
    fn deserialize(
        &mut self,
        zalsa: &mut Zalsa,
        deserializer: &mut dyn erased_serde::Deserializer,
    ) -> Result<(), erased_serde::Error> {
        let deserialize = persistence::DeserializeIngredient {
            zalsa,
            ingredient: self,
        };

        serde::de::DeserializeSeed::deserialize(deserialize, deserializer)
    }
}

fn flatten_cycle_head_dependencies(
    ingredient: &dyn FunctionIngredient,
    zalsa: &Zalsa,
    database_key_index: DatabaseKeyIndex,
    cycle_recovery_strategy: CycleRecoveryStrategy,
    flattened_input_outputs: &mut FxIndexSet<QueryEdge>,
    seen: &mut FxHashSet<DatabaseKeyIndex>,
) {
    let Some(memo) = ingredient.memo(zalsa, database_key_index.key_index()) else {
        return;
    };

    let memo_header = memo.header();

    // Only flatten dependencies of provisional queries, because only those
    // contain cyclic dependencies.
    if !memo_header.may_be_provisional() {
        flattened_input_outputs.insert(QueryEdge::input(database_key_index));
        return;
    }

    // There's nothing to do if we've visited this query before.
    if !seen.insert(database_key_index) {
        return;
    }

    let inputs = memo_header.origin().inputs();

    match cycle_recovery_strategy {
        // Queries with cycle handling already flattened their own dependencies when completing
        // the query. Cycle participants commonly share most of those inputs, often in long
        // contiguous runs, so compare by ordered index to avoid a hash-table lookup for every
        // edge. On a mismatch, `insert_full` preserves insertion order and resynchronizes the
        // expected index.
        CycleRecoveryStrategy::FallbackImmediate | CycleRecoveryStrategy::Fixpoint => {
            let mut expected_index = 0;
            for input in inputs.map(QueryEdge::input) {
                if flattened_input_outputs.get_index(expected_index) == Some(&input) {
                    expected_index += 1;
                } else {
                    let (index, _) = flattened_input_outputs.insert_full(input);
                    expected_index = index + 1;
                }
            }
        }
        // For regular queries, recurse
        CycleRecoveryStrategy::Panic => {
            for input in inputs {
                let ingredient = zalsa.lookup_ingredient(input.ingredient_index());
                ingredient.flatten_cycle_head_dependencies(
                    zalsa,
                    input.key_index(),
                    flattened_input_outputs,
                    seen,
                );
            }
        }
    }
}

impl memo::MemoHeader {
    fn provisional_status(&self) -> ProvisionalStatus<'_> {
        let iteration = self.revisions.iteration();
        let verified_at = self.verified_at.load();

        if self.revisions.verified_final.load(Ordering::Relaxed) {
            ProvisionalStatus::Final {
                iteration,
                verified_at,
            }
        } else {
            ProvisionalStatus::Provisional {
                iteration,
                verified_at,
                cycle_heads: self.cycle_heads(),
            }
        }
    }

    fn set_cycle_iteration_count(
        &self,
        database_key_index: DatabaseKeyIndex,
        iteration: IterationStamp,
    ) {
        self.revisions
            .set_iteration_count(database_key_index, iteration);
    }

    fn finalize_cycle_head(&self) {
        self.revisions.verified_final.store(true, Ordering::Release);
    }

    fn cycle_converged(&self) -> bool {
        self.revisions.cycle_converged()
    }
}

impl<C> std::fmt::Debug for IngredientImpl<C>
where
    C: Configuration,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(std::any::type_name::<Self>())
            .field("index", &self.index)
            .finish()
    }
}

#[cfg(feature = "persistence")]
mod persistence {
    use super::{Configuration, IngredientImpl, Memo};
    use crate::flatten::{Bits, EdgeFlattener, LeafSet};
    use crate::hash::{FxHashMap, FxIndexSet};
    use crate::sync::Arc;
    use crate::plumbing::{MemoIngredientMap, SalsaStructInDb};
    use crate::zalsa::Zalsa;
    use crate::zalsa_local::persistence::PersistentQueryOrigin;
    use crate::zalsa_local::{QueryEdge, QueryOriginRef};
    use crate::{Id, IngredientIndex};

    use serde::de;
    use serde::ser::SerializeMap;

    use std::ptr::NonNull;

    pub struct SerializeIngredient<'db, C>
    where
        C: Configuration,
    {
        pub zalsa: &'db Zalsa,
        pub ingredient: &'db IngredientImpl<C>,
    }

    /// What a memo's origin is saved as: the sets its non-persisted dependencies reach (each
    /// kept once, whoever refers to it) and the edges of its own.
    enum Flat {
        Assigned(crate::key::DatabaseKeyIndex),
        Derived(Vec<LeafSet>, Box<[QueryEdge]>),
        DerivedUntracked(Vec<LeafSet>, Box<[QueryEdge]>),
    }

    impl<C> serde::Serialize for SerializeIngredient<'_, C>
    where
        C: Configuration,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let Self { ingredient, zalsa } = self;

            let mut flattener = EdgeFlattener::default();
            flattener.owner = Some(ingredient.index);
            let mut pending: Vec<(String, &Memo<C>, Flat)> = Vec::new();

            for entry in <C::SalsaStruct<'_> as SalsaStructInDb>::entries(zalsa) {
                let memo_ingredient_index = ingredient
                    .memo_ingredient_indices
                    .get(entry.ingredient_index());

                let memo = ingredient.get_memo_from_table_for(
                    zalsa,
                    entry.key_index(),
                    memo_ingredient_index,
                );

                let Some(memo) = memo.filter(|memo| memo.should_serialize()) else {
                    continue;
                };

                // Flatten the dependencies of this query down to the base inputs.
                let flat = match memo.header.origin() {
                    QueryOriginRef::Derived(edges) => {
                        // SAFETY: Serialization's exclusive storage access prevents database writes.
                        let (sets, own, _) = unsafe { flatten_memo(zalsa, edges, &mut flattener) };
                        Flat::Derived(sets, own)
                    }
                    QueryOriginRef::DerivedUntracked(edges) => {
                        // SAFETY: Serialization's exclusive storage access prevents database writes.
                        let (sets, own, _) = unsafe { flatten_memo(zalsa, edges, &mut flattener) };
                        Flat::DerivedUntracked(sets, own)
                    }
                    QueryOriginRef::Assigned(key) => {
                        let dependency = zalsa.lookup_ingredient(key.ingredient_index());
                        assert!(
                            dependency.is_persistable(),
                            "specified query `{}` must be persistable",
                            dependency.debug_name()
                        );

                        Flat::Assigned(key)
                    }
                };

                // TODO: Group structs by ingredient index into a nested map.
                let key = format!(
                    "{}:{}",
                    entry.ingredient_index().as_u32(),
                    entry.key_index().as_bits()
                );
                pending.push((key, memo, flat));
            }

            // Asked for by the `salsa::explain` target: for the first few memos that depend on a
            // crate the tool added, the chain of queries that leads there. The memos of the lowest
            // keys are the ones made first.
            if ::tracing::enabled!(target: "salsa::explain", ::tracing::Level::DEBUG) {
                let mut shown = 0;
                for (key, memo, flat) in &pending {
                    let (Flat::Derived(sets, own) | Flat::DerivedUntracked(sets, own)) = flat
                    else {
                        continue;
                    };
                    let reaches = sets
                        .iter()
                        .flat_map(|set| flattener.edges_of(set))
                        .chain(own.iter().copied())
                        .any(|edge| crate::flatten::is_low_crate_data(zalsa, edge));
                    // SAFETY: Serialization's exclusive storage access prevents database writes.
                    if reaches
                        && let Some(path) = unsafe {
                            crate::flatten::explain_low_crate_data(zalsa, memo.header.origin().edges())
                        }
                    {
                        ::tracing::debug!(
                            target: "salsa::explain",
                            ingredient = C::DEBUG_NAME,
                            memo = key.as_str(),
                            path = path.as_str(),
                            "a memo depends on a crate the tool added through"
                        );
                        shown += 1;
                        if shown >= 5 {
                            break;
                        }
                    }
                }
            }

            // Each distinct set the memos refer to, once.
            let mut numbers: FxHashMap<*const Bits, u32> = FxHashMap::default();
            let mut table: Vec<Vec<(u32, u32)>> = Vec::new();
            let mut by_kind: std::collections::BTreeMap<&'static str, u64> = Default::default();
            let mut own_edges: u64 = 0;
            let mut set_edges: u64 = 0;
            let mut numbered = |set: &LeafSet, flattener: &EdgeFlattener| -> u32 {
                *numbers.entry(Arc::as_ptr(set)).or_insert_with(|| {
                    for edge in flattener.edges_of(set) {
                        let name = if edge.is_watermark() {
                            "<watermark>"
                        } else {
                            zalsa
                                .lookup_ingredient(edge.key().ingredient_index())
                                .debug_name()
                        };
                        *by_kind.entry(name).or_default() += 1;
                    }
                    let runs = flattener.runs_of(set);
                    set_edges += runs.iter().map(|(_, length)| u64::from(*length)).sum::<u64>();
                    table.push(runs);
                    u32::try_from(table.len() - 1).expect("fewer than 2^32 sets")
                })
            };
            let mut origins: Vec<PersistentQueryOrigin> = Vec::with_capacity(pending.len());
            for (_, _, flat) in &pending {
                origins.push(match flat {
                    Flat::Assigned(key) => PersistentQueryOrigin::assigned(*key),
                    Flat::Derived(sets, own) => {
                        own_edges += own.len() as u64;
                        let sets = sets.iter().map(|set| numbered(set, &flattener)).collect();
                        PersistentQueryOrigin::shared_derived(sets, own.clone())
                    }
                    Flat::DerivedUntracked(sets, own) => {
                        own_edges += own.len() as u64;
                        let sets = sets.iter().map(|set| numbered(set, &flattener)).collect();
                        PersistentQueryOrigin::shared_derived_untracked(sets, own.clone())
                    }
                });
            }

            let count = pending.len();
            // The memos, and the list of edges and the sets they refer to.
            let mut map = serializer.serialize_map(Some(count + 2))?;
            // The list of edges the sets are positions in, then the sets.
            map.serialize_entry("universe", flattener.universe())?;
            map.serialize_entry("sets", &table)?;
            for ((key, memo, _), origin) in pending.into_iter().zip(origins) {
                map.serialize_entry(&key, &memo.with_origin(origin))?;
            }

            if count > 0 {
                let (stats, elapsed) = flattener.report();
                crate::flatten::note_breadth(C::DEBUG_NAME, flattener.breadth(12));
                ::tracing::info!(
                    target: "salsa::persistence",
                    ingredient = C::DEBUG_NAME,
                    memos = count,
                    sets = table.len(),
                    set_edges,
                    set_runs = table.iter().map(|set| set.len() as u64).sum::<u64>(),
                    own_edges,
                    walked = stats.walked,
                    reused = stats.reused,
                    incomplete = stats.incomplete,
                    breadth = flattener.breadth(8),
                    reads_themselves = flattener.direct(8),
                    kinds = ?by_kind,
                    micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX),
                    "flattened the dependencies of the persisted memos"
                );
            }

            map.end()
        }
    }

    /// Flatten the dependency edges of one memo: the sets its non-persisted dependencies
    /// reach, and the edges of its own.
    ///
    /// # Safety
    ///
    /// The caller must prevent database writes throughout the traversal, including query
    /// execution that can reuse slots or modify memos. Concurrent read-only access is allowed.
    unsafe fn flatten_memo(
        zalsa: &Zalsa,
        edges: crate::zalsa_local::QueryEdges<'_>,
        flattener: &mut EdgeFlattener,
    ) -> (Vec<LeafSet>, Box<[QueryEdge]>, crate::flatten::Closure) {
        let mut sets: Vec<LeafSet> = Vec::new();
        let mut own = FxIndexSet::default();
        let mut closure = crate::flatten::Closure::nothing();
        flattener.met_no_memo = false;
        flattener.carried = crate::flatten::Closure::nothing();

        for edge in edges {
            // A memo restored and saved again still has its watermark.
            if edge.is_watermark() {
                own.insert(edge);
                continue;
            }
            // And the sets it refers to, which are saved as the sets they were.
            if edge.is_set() {
                let set = flattener.restored_set(edge.set_number());
                if !sets.iter().any(|known| Arc::ptr_eq(known, &set)) {
                    sets.push(set);
                }
                continue;
            }

            let dependency = zalsa.lookup_ingredient(edge.key().ingredient_index());

            if dependency.is_persistable() {
                // If the dependency will be serialized, we can serialize the edge directly (or
                // the watermark, for an input that rarely changes).
                own.insert(if dependency.watermarked(zalsa, edge.key().key_index()) {
                    QueryEdge::watermark()
                } else {
                    edge
                });
            } else if dependency.as_function().is_some() {
                // SAFETY: The caller prevents database writes throughout the traversal.
                if let Some((set, below)) =
                    unsafe { crate::flatten::leaves_of(zalsa, edge, flattener) }
                {
                    closure.merge(below);
                    if !sets.iter().any(|known| Arc::ptr_eq(known, &set)) {
                        sets.push(set);
                    }
                }
            } else {
                // Otherwise, serialize the minimum edges necessary to cover the dependency.
                // SAFETY: The caller prevents database writes throughout the traversal.
                unsafe {
                    dependency.collect_minimum_serialized_edges(zalsa, edge, &mut own, flattener);
                }
            }
        }

        closure.merge(std::mem::replace(
            &mut flattener.carried,
            crate::flatten::Closure::nothing(),
        ));
        if std::mem::take(&mut flattener.met_no_memo) {
            closure.trusted = false;
        }
        (sets, own.into_iter().collect(), closure)
    }

    /// Verify the persisted memos of `ingredient` that what flattening them sees is enough to
    /// verify: none of the inputs in their sets, and none of the queries between, changed after
    /// they were verified, and what they read could all be seen. It is the walk the save does to
    /// flatten, once, in place of a walk through everything each memo reads for every memo.
    ///
    /// # Safety
    ///
    /// `db` is the database of `ingredient`, and nothing writes to it or runs a query meanwhile.
    pub(super) unsafe fn verify_by_closure<C: Configuration>(
        ingredient: &IngredientImpl<C>,
        zalsa: &Zalsa,
        db: crate::database::RawDatabase<'_>,
    ) {
        if !C::PERSIST {
            return;
        }
        let started = std::time::Instant::now();
        let current = zalsa.current_revision();
        let mut flattener = EdgeFlattener::default();
        flattener.owner = Some(ingredient.index);
        // Whether each set has an edge that may have changed after a revision: found once for all
        // the memos that share the set.
        let mut unchanged: FxHashMap<(*const Bits, crate::Revision), bool> = FxHashMap::default();
        let (mut memos, mut by_closure) = (0u64, 0u64);
        for entry in <C::SalsaStruct<'_> as SalsaStructInDb>::entries(zalsa) {
            let memo_ingredient_index = ingredient
                .memo_ingredient_indices
                .get(entry.ingredient_index());
            let Some(memo) =
                ingredient.get_memo_from_table_for(zalsa, entry.key_index(), memo_ingredient_index)
            else {
                continue;
            };
            let verified_at = memo.header.verified_at.load();
            if !memo.should_serialize() || verified_at == current {
                continue;
            }
            memos += 1;
            let QueryOriginRef::Derived(edges) = memo.header.origin() else {
                continue;
            };
            // SAFETY: The caller prevents database writes and queries running.
            let (sets, own, closure) = unsafe { flatten_memo(zalsa, edges, &mut flattener) };
            if !closure.trusted || closure.changed > verified_at {
                continue;
            }
            if !own
                .iter()
                .all(|edge| crate::flatten::edge_unchanged(db, zalsa, *edge, verified_at))
            {
                continue;
            }
            let sets_unchanged = sets.iter().all(|set| {
                *unchanged
                    .entry((Arc::as_ptr(set), verified_at))
                    .or_insert_with(|| {
                        flattener.edges_of(set).all(|edge| {
                            crate::flatten::edge_unchanged(db, zalsa, edge, verified_at)
                        })
                    })
            });
            if sets_unchanged {
                memo.header.mark_verified_now(zalsa);
                by_closure += 1;
            }
        }
        if memos > 0 {
            ::tracing::info!(
                target: "salsa::persistence",
                ingredient = C::DEBUG_NAME,
                memos,
                verified_from_the_flatten = by_closure,
                left_for_walking = memos - by_closure,
                micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
                "verified the persisted memos that flattening them was enough for"
            );
        }
    }

    pub struct DeserializeIngredient<'db, C>
    where
        C: Configuration,
    {
        pub zalsa: &'db mut Zalsa,
        pub ingredient: &'db mut IngredientImpl<C>,
    }

    impl<'de, C> de::DeserializeSeed<'de> for DeserializeIngredient<'_, C>
    where
        C: Configuration,
    {
        type Value = ();

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_map(self)
        }
    }

    impl<'de, C> de::Visitor<'de> for DeserializeIngredient<'_, C>
    where
        C: Configuration,
    {
        type Value = ();

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a map")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let DeserializeIngredient { zalsa, ingredient } = self;

            // The sets the memos' dependencies are saved as, until the last memo is read.
            let mut _sets = None;
            let mut universe: Vec<QueryEdge> = Vec::new();

            while let Some(key) = access.next_key::<&str>()? {
                if key == "universe" {
                    universe = access.next_value()?;
                    continue;
                }
                if key == "sets" {
                    _sets = Some(crate::flatten::SavedSets::install(
                        std::mem::take(&mut universe),
                        access.next_value()?,
                    ));
                    continue;
                }
                let memo: Memo<C> = access.next_value()?;
                let (ingredient_index, id) = key
                    .split_once(':')
                    .ok_or_else(|| de::Error::custom("invalid database key"))?;

                let ingredient_index = IngredientIndex::new(
                    ingredient_index.parse::<u32>().map_err(de::Error::custom)?,
                );

                let id = Id::from_bits(id.parse::<u64>().map_err(de::Error::custom)?);

                let memo_ingredient_index =
                    ingredient.memo_ingredient_indices.get(ingredient_index);

                // SAFETY: Deserialization holds exclusive database access. The slot was restored
                // before its memos, but its saved revision may differ from the current runtime.
                let memo_table = unsafe { zalsa.table().dyn_memos_unchecked(id) };

                memo_table.insert(
                    memo_ingredient_index,
                    // FIXME: Use `Box::into_non_null` once stable.
                    NonNull::from(Box::leak(Box::new(memo))),
                );
            }

            Ok(())
        }
    }
}
