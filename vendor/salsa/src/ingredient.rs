use std::any::{Any, TypeId};
use std::fmt;

use crate::database::RawDatabase;
use crate::function::{FunctionIngredientRef, VerifyResult};
use crate::hash::{FxHashSet, FxIndexSet};
use crate::sync::Arc;
use crate::table::Table;
use crate::table::memo::MemoTableTypes;
use crate::zalsa::{IngredientIndex, JarKind, Zalsa, transmute_data_mut_ptr, transmute_data_ptr};
use crate::zalsa_local::QueryEdge;
use crate::{DatabaseKeyIndex, Id, Revision};

/// A "jar" is a group of ingredients that are added atomically.
///
/// Each type implementing jar can be added to the database at most once.
pub trait Jar: Any {
    /// Create the ingredients given the index of the first one.
    ///
    /// All subsequent ingredients will be assigned contiguous indices.
    fn create_ingredients(
        zalsa: &mut Zalsa,
        first_index: IngredientIndex,
    ) -> Vec<Box<dyn Ingredient>>;

    /// This returns the [`TypeId`] of the ID struct, that is, the struct that wraps `salsa::Id`
    /// and carry the name of the jar.
    fn id_struct_type_id() -> TypeId;
}

pub struct Location {
    pub file: &'static str,
    pub line: u32,
}

pub trait Ingredient: Any + fmt::Debug + Send + Sync {
    fn debug_name(&self) -> &'static str;
    fn location(&self) -> &'static Location;
    fn jar_kind(&self) -> JarKind;

    /// Has the value for `input` in this ingredient changed after `revision`?
    ///
    /// # Safety
    ///
    /// The passed in database needs to be the same one that the ingredient was created with.
    unsafe fn maybe_changed_after(
        &self,
        zalsa: &Zalsa,
        db: RawDatabase<'_>,
        input: Id,
        revision: Revision,
    ) -> VerifyResult;

    /// Collects the minimum edges necessary to serialize a given dependency edge on this ingredient,
    /// without necessarily serializing the dependency edge itself.
    ///
    /// This generally only returns any transitive input dependencies, i.e. the leaves of the dependency
    /// tree, as most other fine-grained dependencies are covered by the inputs.
    ///
    /// Note that any ingredients returned by this function must be persistable.
    ///
    /// # Safety
    ///
    /// The caller must prevent database writes throughout the traversal, including query
    /// execution that can reuse slots or modify memos. Concurrent read-only access is allowed.
    unsafe fn collect_minimum_serialized_edges(
        &self,
        zalsa: &Zalsa,
        edge: QueryEdge,
        serialized_edges: &mut FxIndexSet<QueryEdge>,
        flattener: &mut crate::flatten::EdgeFlattener,
    );

    /// Returns the function-specific interface for this ingredient, if any.
    #[doc(hidden)]
    fn as_function(&self) -> Option<FunctionIngredientRef<'_>> {
        None
    }

    /// Invoked when the value `output_key` should be marked as valid in the current revision.
    /// This occurs because the value for `executor`, which generated it, was marked as valid
    /// in the current revision.
    fn mark_validated_output(&self, zalsa: &Zalsa, executor: DatabaseKeyIndex, output_key: Id) {
        let _ = (zalsa, executor, output_key);
        unreachable!("only tracked struct and function ingredients can have validatable outputs")
    }

    /// Invoked when the value `stale_output` was output by `executor` in a previous
    /// revision, but was NOT output in the current revision.
    ///
    /// This hook is used to clear out the stale value so others cannot read it.
    fn remove_stale_output(&self, zalsa: &Zalsa, executor: DatabaseKeyIndex, stale_output_key: Id) {
        let _ = (zalsa, executor, stale_output_key);
        unreachable!("only tracked struct ingredients can have stale outputs")
    }

    /// Returns the [`IngredientIndex`] of this ingredient.
    fn ingredient_index(&self) -> IngredientIndex;

    /// Returns true if `reset_for_new_revision` should be called when new revisions start.
    /// Invoked once when ingredient is added and not after that.
    fn requires_reset_for_new_revision(&self) -> bool {
        false
    }

    /// Invoked when a new revision is about to start.
    /// This moment is important because it means that we have an `&mut`-reference to the
    /// database, and hence any pre-existing `&`-references must have expired.
    /// Many ingredients, given an `&'db`-reference to the database,
    /// use unsafe code to return `&'db`-references to internal values.
    /// The backing memory for those values can only be freed once an `&mut`-reference to the
    /// database is created.
    ///
    /// **Important:** to actually receive resets, the ingredient must set
    /// [`IngredientRequiresReset::RESET_ON_NEW_REVISION`] to true.
    fn reset_for_new_revision(&mut self, table: &mut Table) {
        _ = table;
        panic!(
            "Ingredient `{}` set `Ingredient::requires_reset_for_new_revision` to true but does \
            not overwrite `Ingredient::reset_for_new_revision`",
            self.debug_name()
        );
    }

    fn memo_table_types(&self) -> &Arc<MemoTableTypes>;

    fn memo_table_types_mut(&mut self) -> &mut Arc<MemoTableTypes>;

    fn fmt_index(&self, index: Id, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_index(self.debug_name(), index, fmt)
    }
    /// Flattens the dependencies of a query with cycle handling that participates in a cycle.
    ///
    /// This query recursively walks the dependency graph of `id` and flattens input dependencies
    /// on provisional queries into `flattened_input_outputs`. Outputs aren't flattened because
    /// outputs are owned by the creating query and not the cycle head.
    ///
    /// Flattening the dependencies is necessary because the memo's dependency graph only captures
    /// the dependencies from the last iteration. It also ensures that the dependency graph doesn't
    /// contain any cycles.
    ///
    /// See `cycle_dependency_order_different_entry_queries` and `cycle_left_recursive_query`.
    fn flatten_cycle_head_dependencies(
        &self,
        zalsa: &Zalsa,
        id: Id,
        flattened_input_outputs: &mut FxIndexSet<QueryEdge>,
        seen: &mut FxHashSet<DatabaseKeyIndex>,
    );

    /// What values were accumulated during the creation of the value at `key_index`
    /// (if any).
    ///
    /// # Safety
    ///
    /// The passed in database needs to be the same one that the ingredient was created with.
    #[cfg(feature = "accumulator")]
    unsafe fn accumulated<'db>(
        &'db self,
        db: RawDatabase<'db>,
        key_index: Id,
    ) -> (
        Option<&'db crate::accumulator::accumulated_map::AccumulatedMap>,
        crate::accumulator::accumulated_map::InputAccumulatedValues,
    ) {
        let _ = (db, key_index);
        (
            None,
            crate::accumulator::accumulated_map::InputAccumulatedValues::Empty,
        )
    }

    /// Returns memory usage information about any instances of the ingredient,
    /// if applicable.
    ///
    /// # Safety
    ///
    /// The database must be the one that owns this ingredient. The caller must ensure
    /// exclusive access to its storage for the duration of this call: no query
    /// execution or mutation may overlap the traversal.
    #[cfg(feature = "salsa_unstable")]
    unsafe fn memory_usage(
        &self,
        _db: &dyn crate::Database,
    ) -> Option<Vec<crate::database::SlotInfo>> {
        None
    }

    /// Whether this ingredient will be persisted with the database.
    fn is_persistable(&self) -> bool {
        false
    }

    /// How many of this ingredient's memos hold a value in memory, and, if `drop`, drop those
    /// values (a memo whose value cannot be decoded again from bytes is computed again when
    /// something reads it). Memos whose value cannot be computed again are left alone.
    /// If `inputs` is present, visit only memos keyed directly by those IDs.
    ///
    /// # Safety
    ///
    /// The database must be the one that owns this ingredient, and the caller must have exclusive
    /// access to its storage for the duration of the call.
    ///
    /// A persisted query's values are dropped only if `include_persisted`: a persisted memo with no
    /// value is not in the state a run saves.
    unsafe fn values_in_memory(
        &self,
        _zalsa: &Zalsa,
        _drop: bool,
        _include_persisted: bool,
        _inputs: Option<&FxHashSet<Id>>,
    ) -> u64 {
        0
    }

    /// Free superseded allocations at an exclusive boundary without a revision or LRU pass.
    fn drain_deferred_memos(&mut self) -> usize {
        0
    }

    /// Retire exactly one indexed function/input pair; never enumerate or refresh inputs.
    ///
    /// # Safety
    /// The caller holds exclusive access to this ingredient's storage.
    unsafe fn retire_owned_value(
        &self,
        _zalsa: &Zalsa,
        _id: Id,
        _store: Option<&dyn crate::spill::SpillStore>,
        _counters: &crate::spill::SpillCounters,
        _report: &mut crate::OwnerRetirementReport,
    ) {
    }

    /// Write the values of this ingredient's persisted memos to `store` and drop them from memory
    /// (see [`crate::spill`]).
    /// If `inputs` is present, visit only memos keyed directly by those IDs.
    ///
    /// # Safety
    ///
    /// The database must be the one that owns this ingredient, and the caller must have exclusive
    /// access to its storage for the duration of the call: no query may run, and no reference to a
    /// memo's value may be held.
    unsafe fn spill_persisted(
        &self,
        _zalsa: &Zalsa,
        _store: &dyn crate::spill::SpillStore,
        _counters: &crate::spill::SpillCounters,
        _inputs: Option<&FxHashSet<Id>>,
    ) {
    }

    /// Verify, at the current revision, every persisted memo of this ingredient that was last
    /// verified at an earlier one, so that what is saved of it (its dependencies as they are now)
    /// goes with the revision they are valid at. A query a memo reads may have run again since the
    /// memo was verified, onto inputs that did not exist then: the memo is still valid, but
    /// saved with those inputs and its old revision it would be found out of date.
    ///
    /// # Safety
    ///
    /// The database must be the one the ingredient was created with.
    #[cfg(feature = "persistence")]
    unsafe fn verify_persisted(
        &self,
        _zalsa: &Zalsa,
        _db: RawDatabase<'_>,
        _part: usize,
        _parts: usize,
    ) {
    }

    /// Verify the persisted memos that what flattening them sees is enough to verify (see
    /// `flatten::Closure`), at the current revision. It runs no query, so ingredients can do it
    /// on several threads at once; the rest are verified by `verify_persisted`.
    ///
    /// # Safety
    ///
    /// The database must be the one the ingredient was created with, and nothing may write to it
    /// or run a query that could replace a memo while this runs.
    #[cfg(feature = "persistence")]
    unsafe fn verify_persisted_by_closure(&self, _zalsa: &Zalsa, _db: RawDatabase<'_>) {}

    /// Set up a tracked function from the views the database has registered, so that a memo that
    /// depends on it can be verified before anything has called it. Does nothing for the others,
    /// and for a function whose view has not been registered.
    fn init_view_caster(&self, _zalsa: &Zalsa) {}

    /// Whether a persisted memo's dependency on `input` is saved as the watermark edge instead:
    /// the input is of the durability that rarely changes, and the memo is out of date whenever
    /// any such input has changed, without saying which.
    fn watermarked(&self, _zalsa: &Zalsa, _input: Id) -> bool {
        false
    }

    /// Whether there is data to serialize for this ingredient.
    ///
    /// If this returns `false`, the ingredient will not be serialized, even if `is_persistable`
    /// returns `true`.
    fn should_serialize(&self, _zalsa: &Zalsa) -> bool {
        false
    }

    /// Whether every memo this ingredient would serialize is verified as of the current revision.
    /// A state is saved with canonical revision stamps only when every ingredient says so (see
    /// `stamps`); an ingredient with no memos has nothing that could be out of date.
    fn all_verified_now(&self, _zalsa: &Zalsa) -> bool {
        true
    }

    /// Serialize the ingredient.
    ///
    /// This function should invoke the provided callback with a reference to an object implementing [`erased_serde::Serialize`].
    ///
    /// # Safety
    ///
    /// While this method takes an immutable reference to the database, the serializer must hold
    /// exclusive access to its storage until the callback returns.
    // See <https://github.com/dtolnay/erased-serde/issues/113> for why this callback signature is necessary, instead
    // of providing an `erased_serde::Serializer` directly.
    #[cfg(feature = "persistence")]
    unsafe fn serialize<'db>(
        &'db self,
        _zalsa: &'db Zalsa,
        _f: &mut dyn FnMut(&dyn erased_serde::Serialize),
    ) {
        unimplemented!("called `serialize` on ingredient where `should_serialize` returns `false`")
    }

    /// Deserialize the ingredient.
    #[cfg(feature = "persistence")]
    fn deserialize(
        &mut self,
        _zalsa: &mut Zalsa,
        _deserializer: &mut dyn erased_serde::Deserializer,
    ) -> Result<(), erased_serde::Error> {
        unimplemented!(
            "called `deserialize` on ingredient where `should_serialize` returns `false`"
        )
    }
}

impl dyn Ingredient {
    /// Equivalent to the `downcast` method on `Any`.
    ///
    /// Because we do not have dyn-downcasting support, we need this workaround.
    pub fn assert_type<T: Any>(&self) -> &T {
        assert_eq!(
            self.type_id(),
            TypeId::of::<T>(),
            "ingredient `{self:?}` is not of type `{}`",
            std::any::type_name::<T>()
        );

        // SAFETY: We know that the underlying data pointer
        // refers to a value of type T because of the `TypeId` check above.
        unsafe { transmute_data_ptr(self) }
    }

    /// Equivalent to the `downcast` methods on `Any`.
    ///
    /// Because we do not have dyn-downcasting support, we need this workaround.
    ///
    /// # Safety
    ///
    /// The contained value must be of type `T`.
    pub unsafe fn assert_type_unchecked<T: Any>(&self) -> &T {
        debug_assert_eq!(
            self.type_id(),
            TypeId::of::<T>(),
            "ingredient `{self:?}` is not of type `{}`",
            std::any::type_name::<T>()
        );

        // SAFETY: Guaranteed by caller.
        unsafe { transmute_data_ptr(self) }
    }

    /// Equivalent to the `downcast` method on `Any`.
    ///
    /// Because we do not have dyn-downcasting support, we need this workaround.
    pub fn assert_type_mut<T: Any>(&mut self) -> &mut T {
        assert_eq!(
            Any::type_id(self),
            TypeId::of::<T>(),
            "ingredient `{self:?}` is not of type `{}`",
            std::any::type_name::<T>()
        );

        // SAFETY: We know that the underlying data pointer
        // refers to a value of type T because of the `TypeId` check above.
        unsafe { transmute_data_mut_ptr(self) }
    }
}

/// A helper function to show human readable fmt.
pub(crate) fn fmt_index(debug_name: &str, id: Id, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(fmt, "{debug_name}({id:?})")
}
