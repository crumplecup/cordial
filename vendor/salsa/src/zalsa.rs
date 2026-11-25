use std::any::{Any, TypeId};
use std::hash::BuildHasherDefault;
use std::panic::RefUnwindSafe;

use hashbrown::HashMap;
use rustc_hash::FxHashMap;

use crate::hash::{FxHashSet, TypeIdHasher};
use crate::ingredient::{Ingredient, Jar};
use crate::plumbing::SalsaStructInDb;
use crate::runtime::Runtime;
use crate::table::Table;
use crate::table::memo::MemoTableWithTypes;
use crate::views::Views;
use crate::zalsa_local::ZalsaLocal;
use crate::{Database, DatabaseKeyIndex, Durability, Id, Revision};

/// Internal plumbing trait.
///
/// [`ZalsaDatabase`] is created automatically when [`#[salsa::db]`](`crate::db`)
/// is attached to a database struct. it Contains methods that give access
/// to the internal data from the `storage` field.
///
/// # Safety
///
/// The system assumes this is implemented by a salsa procedural macro
/// which makes use of private data from the [`Storage`](`crate::storage::Storage`) struct.
/// Do not implement this yourself, instead, apply the [`#[salsa::db]`](`crate::db`) macro
/// to your database.
pub unsafe trait ZalsaDatabase: Any {
    /// Plumbing method: access both zalsa and zalsa-local at once.
    /// More efficient if you need both as it does only a single vtable dispatch.
    #[doc(hidden)]
    fn zalsas(&self) -> (&Zalsa, &ZalsaLocal) {
        (self.zalsa(), self.zalsa_local())
    }

    /// Plumbing method: Access the internal salsa methods.
    #[doc(hidden)]
    fn zalsa(&self) -> &Zalsa;

    /// Plumbing method: Access the internal salsa methods for mutating the database.
    ///
    /// **WARNING:** Triggers cancellation to other database handles.
    /// This can lead to deadlock!
    #[doc(hidden)]
    fn zalsa_mut(&mut self) -> &mut Zalsa;

    /// Exclusive access for metadata inspection, without changing query state.
    /// Implementations may avoid cancellation when this is the only handle.
    /// Callers must not execute queries or mutate memo values through this access.
    #[doc(hidden)]
    fn zalsa_mut_for_inspection(&mut self) -> &mut Zalsa {
        self.zalsa_mut()
    }

    /// Access the thread-local state associated with this database
    #[doc(hidden)]
    fn zalsa_local(&self) -> &ZalsaLocal;
}

pub fn views<Db: ?Sized + Database>(db: &Db) -> &Views {
    db.zalsa().views()
}
/// Nonce type representing the underlying database storage.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg(not(feature = "inventory"))]
pub struct StorageNonce;

static SPILL_STORAGES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

// Generator for storage nonces.
#[cfg(not(feature = "inventory"))]
static NONCE: crate::nonce::NonceGenerator<StorageNonce> = crate::nonce::NonceGenerator::new();

/// An ingredient index identifies a particular [`Ingredient`] in the database.
///
/// The database contains a number of jars, and each jar contains a number of ingredients.
/// Each ingredient is given a unique index as the database is being created.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "persistence", serde(transparent))]
pub struct IngredientIndex(u32);

impl IngredientIndex {
    /// The maximum supported ingredient index.
    ///
    /// This reserves one bit for an optional tag.
    pub(crate) const MAX_INDEX: u32 = 0x7FFF_FFFF;

    /// Create an ingredient index from a `u32`.
    pub(crate) fn new(v: u32) -> Self {
        assert!(v <= Self::MAX_INDEX);
        Self(v)
    }

    /// Create an ingredient index from a `u32`, without performing validating
    /// that the index is valid.
    ///
    /// # Safety
    ///
    /// The index must be less than or equal to `IngredientIndex::MAX_INDEX`.
    pub(crate) const unsafe fn new_unchecked(v: u32) -> Self {
        Self(v)
    }

    /// Convert the ingredient index back into a `u32`.
    pub(crate) const fn as_u32(self) -> u32 {
        self.0
    }

    pub fn successor(self, index: usize) -> Self {
        IngredientIndex(self.0 + 1 + index as u32)
    }

    #[inline(always)]
    pub(crate) fn at_offset(self, offset: usize) -> Self {
        IngredientIndex(self.0 + offset as u32)
    }

    /// Returns a new `IngredientIndex` with the tag bit set to the provided value.
    pub(crate) const fn with_tag(mut self, tag: bool) -> IngredientIndex {
        self.0 &= Self::MAX_INDEX;
        self.0 |= (tag as u32) << 31;
        self
    }

    /// Returns the value of the tag bit.
    pub(crate) const fn tag(self) -> bool {
        self.0 & !Self::MAX_INDEX != 0
    }
}

/// A special secondary index *just* for ingredients that attach
/// "memos" to salsa structs (currently: just tracked functions).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct MemoIngredientIndex(u32);

impl MemoIngredientIndex {
    pub(crate) fn from_usize(u: usize) -> Self {
        assert!(u <= u32::MAX as usize);
        MemoIngredientIndex(u as u32)
    }

    #[inline]
    pub(crate) fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// The "plumbing interface" to the Salsa database. Stores all the ingredients and other data.
///
/// **NOT SEMVER STABLE.**
pub struct Zalsa {
    views_of: Views,

    /// Tells this database's spilled values from those of the others in the process.
    spill_storage: u32,

    #[cfg(not(feature = "inventory"))]
    nonce: crate::nonce::Nonce<StorageNonce>,

    /// Values that belong to this storage and are not a query's result, by their type: made when first asked for.
    extensions: std::sync::Mutex<FxHashMap<TypeId, std::sync::Arc<dyn Any + Send + Sync>>>,

    /// Map from the [`IngredientIndex::as_usize`][] of a salsa struct to a list of
    /// [ingredient-indices](`IngredientIndex`) for tracked functions that have this salsa struct
    /// as input.
    memo_ingredient_indices: Vec<Vec<IngredientIndex>>,

    /// Map from the type-id of an `impl Jar` to the index of its first ingredient.
    jar_map: HashMap<TypeId, IngredientIndex, BuildHasherDefault<TypeIdHasher>>,

    /// A map from the `IngredientIndex` to the `TypeId` of its ID struct.
    ///
    /// Notably this is not the reverse mapping of `jar_map`.
    ingredient_to_id_struct_type_id_map: FxHashMap<IngredientIndex, TypeId>,

    /// Vector of ingredients.
    ingredients_vec: Vec<Box<dyn Ingredient>>,

    /// Indices of ingredients that require reset when a new revision starts.
    ingredients_requiring_reset: Vec<IngredientIndex>,

    /// The runtime for this particular salsa database handle.
    /// Each handle gets its own runtime, but the runtimes have shared state between them.
    runtime: Runtime,

    event_callback: Option<Box<dyn Fn(crate::Event) + Send + Sync>>,

    pub(crate) owner_index: std::sync::Arc<crate::ownership::OwnerIndex>,
    pub(crate) ownership_resolvers: crate::ownership::Resolvers,

    /// The queries whose value has a saved fingerprint, with the query that is the fingerprint: a
    /// memo that read the first is saved as depending on the second instead of on everything the
    /// first read (see `flatten::cutoff_edge`).
    cutoffs: std::sync::RwLock<Vec<(IngredientIndex, IngredientIndex)>>,

    /// What a save has worked out of the dependencies of the memos so far, kept from one ingredient
    /// to the next while the save lasts (see `flatten`), and dropped when the next save begins.
    save_flattener: std::sync::Mutex<Option<crate::flatten::EdgeFlattener>>,

    /// How the state this database was restored from cut each ingredient into chunks, by ingredient
    /// index, for the save that follows (see `chunks`).
    chunk_plan: std::sync::Mutex<crate::hash::FxHashMap<u32, std::sync::Arc<Vec<crate::chunks::PlannedChunk>>>>,
}

/// All fields on Zalsa are locked behind [`Mutex`]es and [`RwLock`]s and cannot enter
/// inconsistent states. The contents of said fields are largely ID mappings, with the exception
/// of [`Runtime::dependency_graph`]. However, [`Runtime::dependency_graph`] does not
/// invoke any queries and as such there will be no panic from code downstream of Salsa. It can only
/// panic if an assertion inside of Salsa fails.
impl RefUnwindSafe for Zalsa {}

impl Zalsa {
    pub(crate) fn new<Db: Database>(
        event_callback: Option<Box<dyn Fn(crate::Event) + Send + Sync + 'static>>,
        jars: Vec<ErasedJar>,
    ) -> Self {
        let mut zalsa = Self {
            views_of: Views::new::<Db>(),
            jar_map: HashMap::default(),
            ingredient_to_id_struct_type_id_map: Default::default(),
            ingredients_vec: Vec::new(),
            ingredients_requiring_reset: Vec::new(),
            spill_storage: SPILL_STORAGES.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            runtime: Runtime::default(),
            memo_ingredient_indices: Default::default(),
            event_callback,
            owner_index: Default::default(),
            ownership_resolvers: Default::default(),
            cutoffs: std::sync::RwLock::new(Vec::new()),
            save_flattener: std::sync::Mutex::new(None),
            chunk_plan: std::sync::Mutex::new(Default::default()),
            #[cfg(not(feature = "inventory"))]
            nonce: NONCE.nonce(),
            extensions: std::sync::Mutex::new(FxHashMap::default()),
        };

        // Collect and initialize all registered ingredients.
        #[cfg(feature = "inventory")]
        let mut jars = inventory::iter::<ErasedJar>()
            .copied()
            .chain(jars)
            .collect::<Vec<_>>();

        #[cfg(not(feature = "inventory"))]
        let mut jars = jars;

        // The keyed ingredient belongs to every database.
        jars.push(ErasedJar::erase::<crate::keyed::KeyedHasJar>());

        // Ensure structs are initialized before tracked functions.
        //
        // We also further sort by debug name, to maintain a consistent ordering across
        // builds.
        jars.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.type_name().cmp(b.type_name())));

        for jar in jars {
            zalsa.insert_jar(jar);
        }

        zalsa
    }

    /// The value of type `T` that belongs to this storage, made with `T::default()` the first time it is asked for.
    /// It is shared by every handle of the storage and dropped with it.
    pub fn extension<T: Any + Send + Sync + Default>(&self) -> std::sync::Arc<T> {
        let mut extensions = self.extensions.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let found = extensions
            .entry(TypeId::of::<T>())
            .or_insert_with(|| std::sync::Arc::new(T::default()) as std::sync::Arc<dyn Any + Send + Sync>)
            .clone();
        found
            .downcast::<T>()
            .unwrap_or_else(|_| unreachable!("an extension is stored under its own type"))
    }

    #[cfg(not(feature = "inventory"))]
    pub(crate) fn nonce(&self) -> crate::nonce::Nonce<StorageNonce> {
        self.nonce
    }

    pub(crate) fn spill_storage(&self) -> u32 {
        self.spill_storage
    }

    pub(crate) fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    pub(crate) fn runtime_mut(&mut self) -> &mut Runtime {
        &mut self.runtime
    }

    /// Returns the [`Table`] used to store the value of salsa structs
    #[inline]
    pub fn table(&self) -> &Table {
        self.runtime.table()
    }

    /// Returns a mutable reference to the [`Table`] used to store the value of salsa structs
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn table_mut(&mut self) -> &mut Table {
        self.runtime.table_mut()
    }

    /// Returns the [`MemoTable`][] for the salsa struct with the given id
    pub(crate) fn memo_table_for<T: SalsaStructInDb>(&self, id: Id) -> MemoTableWithTypes<'_> {
        // SAFETY: We are supplying the correct current revision.
        unsafe { T::memo_table(self, id, self.current_revision()) }
    }

    /// Returns the ingredient at the given index, or panics if it is out-of-bounds.
    #[inline]
    pub fn lookup_ingredient(&self, index: IngredientIndex) -> &dyn Ingredient {
        self.ingredients_vec[index.as_u32() as usize].as_ref()
    }

    /// Returns the ingredient at the given index.
    ///
    /// # Safety
    ///
    /// The index must be in-bounds.
    #[inline]
    pub unsafe fn lookup_ingredient_unchecked(&self, index: IngredientIndex) -> &dyn Ingredient {
        // SAFETY: Guaranteed by caller.
        unsafe {
            self.ingredients_vec
                .get_unchecked(index.as_u32() as usize)
                .as_ref()
        }
    }

    pub(crate) fn ingredient_index_for_memo(
        &self,
        struct_ingredient_index: IngredientIndex,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> IngredientIndex {
        self.memo_ingredient_indices[struct_ingredient_index.as_u32() as usize]
            [memo_ingredient_index.as_usize()]
    }

    /// The query whose value is the saved fingerprint of the value of `query`, if there is one.
    pub(crate) fn cutoff_of(&self, query: IngredientIndex) -> Option<IngredientIndex> {
        self.cutoffs
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .find_map(|(known, fingerprint)| (*known == query).then_some(*fingerprint))
    }

    /// Whether some query's value is saved as the value of `ingredient`: its own walk goes through the
    /// query it fingerprints, so what it reaches is not what every other ingredient's does.
    pub(crate) fn is_fingerprint(&self, ingredient: IngredientIndex) -> bool {
        self.cutoffs
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .any(|(_, fingerprint)| *fingerprint == ingredient)
    }

    /// Say how the state this database was restored from cut the ingredients into chunks.
    pub(crate) fn set_chunk_plan(&self, plan: Vec<(u32, Vec<crate::chunks::PlannedChunk>)>) {
        *self
            .chunk_plan
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = plan
            .into_iter()
            .map(|(index, chunks)| (index, std::sync::Arc::new(chunks)))
            .collect();
    }

    /// How the last state cut the ingredient `index`, if it is said.
    pub(crate) fn chunk_plan_of(
        &self,
        index: IngredientIndex,
    ) -> Option<std::sync::Arc<Vec<crate::chunks::PlannedChunk>>> {
        self.chunk_plan
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&index.as_u32())
            .cloned()
    }

    /// Begin a save: what an earlier one worked out is dropped.
    pub(crate) fn begin_save(&self) {
        *self
            .save_flattener
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    /// The flattener the save in progress shares among the ingredients whose walk does not depend on
    /// which of them it is for, taken (so it is not shared with itself) and to be given back with
    /// [`Self::return_save_flattener`].
    pub(crate) fn take_save_flattener(&self) -> crate::flatten::EdgeFlattener {
        self.save_flattener
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .unwrap_or_default()
    }

    pub(crate) fn return_save_flattener(&self, flattener: crate::flatten::EdgeFlattener) {
        *self
            .save_flattener
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(flattener);
    }

    /// Say that the value of `fingerprint` stands for the value of `query`.
    pub(crate) fn register_cutoff(&self, query: IngredientIndex, fingerprint: IngredientIndex) {
        let mut cutoffs = self
            .cutoffs
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !cutoffs.iter().any(|(known, _)| *known == query) {
            cutoffs.push((query, fingerprint));
        }
    }

    #[allow(unused)]
    pub(crate) fn ingredients(&self) -> impl Iterator<Item = &dyn Ingredient> {
        self.ingredients_vec
            .iter()
            .map(|ingredient| ingredient.as_ref())
    }

    pub(crate) fn drain_deferred_memos(&mut self) -> usize {
        self.ingredients_vec
            .iter_mut()
            .map(|ingredient| ingredient.drain_deferred_memos())
            .sum()
    }

    /// Starts unwinding the stack if the current revision is cancelled.
    ///
    /// This method can be called by query implementations that perform
    /// potentially expensive computations, in order to speed up propagation of
    /// cancellation.
    ///
    /// Cancellation will automatically be triggered by salsa on any query
    /// invocation.
    #[inline]
    pub(crate) fn unwind_if_revision_cancelled(&self, zalsa_local: &ZalsaLocal) {
        self.event(&|| crate::Event::new(crate::EventKind::WillCheckCancellation));
        if zalsa_local.should_trigger_local_cancellation() {
            zalsa_local.unwind_cancelled();
        }
        if self.runtime().load_cancellation_flag() {
            zalsa_local.unwind_pending_write();
        }
    }

    pub(crate) fn next_memo_ingredient_index(
        &mut self,
        struct_ingredient_index: IngredientIndex,
        ingredient_index: IngredientIndex,
    ) -> MemoIngredientIndex {
        let memo_ingredients = &mut self.memo_ingredient_indices;
        let idx = struct_ingredient_index.as_u32() as usize;
        let memo_ingredients = if let Some(memo_ingredients) = memo_ingredients.get_mut(idx) {
            memo_ingredients
        } else {
            memo_ingredients.resize_with(idx + 1, Vec::new);
            memo_ingredients.get_mut(idx).unwrap()
        };
        let mi = MemoIngredientIndex::from_usize(memo_ingredients.len());
        memo_ingredients.push(ingredient_index);

        mi
    }
}

/// Semver unstable APIs used by the macro expansions
impl Zalsa {
    /// **NOT SEMVER STABLE**
    pub fn views(&self) -> &Views {
        &self.views_of
    }

    /// **NOT SEMVER STABLE**
    #[inline]
    pub fn lookup_page_type_id(&self, id: Id) -> TypeId {
        let ingredient_index = self.ingredient_index(id);
        *self
            .ingredient_to_id_struct_type_id_map
            .get(&ingredient_index)
            .expect("should have the ingredient index available")
    }

    /// **NOT SEMVER STABLE**
    #[doc(hidden)]
    #[inline]
    pub fn lookup_jar_by_type<J: Jar>(&self) -> IngredientIndex {
        let jar_type_id = TypeId::of::<J>();

        *self.jar_map.get(&jar_type_id).unwrap_or_else(|| {
            panic!(
                "ingredient `{}` was not registered",
                std::any::type_name::<J>()
            )
        })
    }

    /// Type-erased jar lookup used by shared ingredient cache initialization.
    pub(crate) fn lookup_jar_by_type_id(
        &self,
        jar_type_id: TypeId,
        jar_type_name: &'static str,
    ) -> IngredientIndex {
        *self
            .jar_map
            .get(&jar_type_id)
            .unwrap_or_else(|| panic!("ingredient `{jar_type_name}` was not registered"))
    }

    fn insert_jar(&mut self, jar: ErasedJar) {
        let jar_type_id = (jar.type_id)();

        let index = IngredientIndex::new(self.ingredients_vec.len() as u32);

        if self.jar_map.contains_key(&jar_type_id) {
            return;
        }

        let ingredients = (jar.create_ingredients)(self, index);
        for ingredient in ingredients {
            let expected_index = ingredient.ingredient_index();
            if ingredient.requires_reset_for_new_revision() {
                self.ingredients_requiring_reset.push(expected_index);
            }

            crate::flatten::note_ingredient_name(expected_index, ingredient.debug_name());
            self.ingredients_vec.push(ingredient);

            let actual_index = self.ingredients_vec.len() - 1;
            assert_eq!(
                expected_index.as_u32() as usize,
                actual_index,
                "ingredient `{:?}` was predicted to have index `{:?}` but actually has index `{:?}`",
                self.ingredients_vec[actual_index],
                expected_index.as_u32(),
                actual_index,
            );
        }

        self.jar_map.insert(jar_type_id, index);

        self.ingredient_to_id_struct_type_id_map
            .insert(index, (jar.id_struct_type_id)());
    }

    /// **NOT SEMVER STABLE**
    #[doc(hidden)]
    pub fn lookup_ingredient_mut(
        &mut self,
        index: IngredientIndex,
    ) -> (&mut dyn Ingredient, &mut Runtime) {
        let index = index.as_u32() as usize;
        let ingredient = self
            .ingredients_vec
            .get_mut(index)
            .unwrap_or_else(|| panic!("index `{index}` is uninitialized"));
        (ingredient.as_mut(), &mut self.runtime)
    }

    /// **NOT SEMVER STABLE**
    #[doc(hidden)]
    pub fn take_ingredient(&mut self, index: IngredientIndex) -> Box<dyn Ingredient> {
        self.ingredients_vec.remove(index.as_u32() as usize)
    }

    /// **NOT SEMVER STABLE**
    #[doc(hidden)]
    pub fn replace_ingredient(&mut self, index: IngredientIndex, ingredient: Box<dyn Ingredient>) {
        self.ingredients_vec
            .insert(index.as_u32() as usize, ingredient);
    }

    /// **NOT SEMVER STABLE**
    #[doc(hidden)]
    #[inline]
    pub fn current_revision(&self) -> Revision {
        self.runtime.current_revision()
    }

    /// **NOT SEMVER STABLE**
    #[doc(hidden)]
    #[inline]
    pub fn last_changed_revision(&self, durability: Durability) -> Revision {
        self.runtime.last_changed_revision(durability)
    }

    /// **NOT SEMVER STABLE**
    /// Triggers a new revision.
    #[doc(hidden)]
    pub fn new_revision(&mut self) -> Revision {
        let new_revision = self.runtime.new_revision();
        #[cfg(feature = "detailed-trace")]
        let _span = crate::tracing::debug_span!("new_revision", ?new_revision).entered();

        for ingredient in &self.ingredients_requiring_reset {
            self.ingredients_vec[ingredient.as_u32() as usize]
                .reset_for_new_revision(self.runtime.table_mut());
        }

        new_revision
    }

    /// **NOT SEMVER STABLE**
    ///
    /// For each query, how many memos hold a value in memory, and, if `drop`, drop those values
    /// of the queries `only` names (all if it is empty), one query at a time, calling `after` with
    /// the query's name and the count when each is done, so that what that freed can be measured.
    #[doc(hidden)]
    pub fn values_in_memory(
        &mut self,
        drop: bool,
        include_persisted: bool,
        only: &[&str],
        after: impl FnMut(&'static str, u64),
    ) {
        self.values_in_memory_matching(drop, include_persisted, only, None, after);
    }

    /// Collect exact keys and the tracked outputs of queries keyed by them, transitively.
    /// Only existing memo producer records are read; no queries execute or values decode.
    /// Does not follow read dependencies, interned fields, or composite arguments. Missing
    /// producer memos add no keys. All keys must belong to this database.
    #[doc(hidden)]
    pub fn tracked_output_keys(&mut self, inputs: &[Id]) -> Vec<Id> {
        let mut seen = FxHashSet::default();
        let mut keys: Vec<_> = inputs.iter().copied().filter(|id| seen.insert(*id)).collect();
        let mut cursor = 0;
        while let Some(&key) = keys.get(cursor) {
            let index = self.ingredient_index(key).as_u32() as usize;
            // Struct ingredients with no registered queries may have no mapping entry.
            for &ingredient in self.memo_ingredient_indices.get(index).into_iter().flatten() {
                let Some(function) = self.lookup_ingredient(ingredient).as_function() else {
                    continue;
                };
                // SAFETY: Exclusive storage access prevents queries, slot reuse and memo
                // replacement. The mapping lists only queries compatible with this key.
                let Some(memo) = (unsafe { function.memo_unchecked(self, key) }) else {
                    continue;
                };
                for output in memo.header().tracked_output_ids() {
                    if seen.insert(output) {
                        keys.push(output);
                    }
                }
            }
            cursor += 1;
        }
        keys
    }

    /// Every key the named queries hold a memo for, whatever the memo's state. The queries are named by
    /// their debug names, as in [`Self::cached_query_keys`]. Keys are the ids of the queries' own inputs: a query
    /// of several arguments, or of one that is not a salsa struct, is keyed by an interned tuple.
    #[doc(hidden)]
    pub fn memo_keys(&mut self, only: &[&str]) -> Vec<Id> {
        let mut keys = Vec::new();
        for ingredient in self.ingredients() {
            if !only.contains(&ingredient.debug_name()) {
                continue;
            }
            if let Some(function) = ingredient.as_function() {
                keys.extend(function.memo_keys(self));
            }
        }
        keys
    }

    /// How many slots each struct ingredient named holds (see `Database::struct_slots`).
    #[doc(hidden)]
    pub fn struct_slots(&self, only: &[&str]) -> Vec<(&'static str, u64)> {
        self.ingredients()
            .filter(|ingredient| only.contains(&ingredient.debug_name()))
            .filter_map(|ingredient| Some((ingredient.debug_name(), ingredient.slot_count(self)?)))
            .collect()
    }

    /// Remove every memo keyed directly by these ids (see `Database::forget_memos_for_inputs`).
    #[doc(hidden)]
    pub fn forget_memos_for_inputs(&mut self, inputs: &[Id]) -> u64 {
        let mut planned = Vec::new();
        for &key in inputs {
            let index = self.ingredient_index(key).as_u32() as usize;
            for &ingredient in self.memo_ingredient_indices.get(index).into_iter().flatten() {
                let Some(function) = self.lookup_ingredient(ingredient).as_function() else {
                    continue;
                };
                planned.push((
                    DatabaseKeyIndex::new(ingredient, key),
                    function.memo_index_for(self, key),
                ));
            }
        }
        self.forget_planned(planned)
    }

    /// Remove every memo of a query whose value changed after `revision` (see
    /// `Database::forget_memos_changed_after`).
    #[doc(hidden)]
    pub fn forget_memos_changed_after(&mut self, revision: Revision) -> u64 {
        let mut planned = Vec::new();
        let mut tainted = FxHashMap::default();
        for ingredient in self.ingredients() {
            let Some(function) = ingredient.as_function() else {
                continue;
            };
            for key in function.memo_keys(self) {
                // SAFETY: `&mut self` is exclusive access to the storage: no query runs and nothing
                // borrows a memo.
                let Some(memo) = (unsafe { function.memo_unchecked(self, key) }) else {
                    continue;
                };
                // A query's own result, which running it again makes again: not an input's field, and not
                // one whose value was handed to it.
                // A query that made tracked structs keeps its memo: they are what ids held elsewhere name, and
                // they are the same ones when the query runs again only because the memo is there.
                let closure = memo.header().closure();
                let makes_structs = memo.header().tracked_output_ids().next().is_some();
                // A memo that was made for the first time after `revision` has a value that no earlier state saw;
                // it is the flipped state's only if what it read changed. One that read nothing that changed holds
                // the value the earlier state would give, and is kept.
                if closure.trusted
                    && closure.changed > revision
                    && !makes_structs
                    && self.depends_on_a_change_after(
                        DatabaseKeyIndex::new(ingredient.ingredient_index(), key),
                        revision,
                        &mut tainted,
                    )
                {
                    planned.push((
                        DatabaseKeyIndex::new(ingredient.ingredient_index(), key),
                        function.memo_index_for(self, key),
                    ));
                }
            }
        }
        self.forget_planned(planned)
    }

    /// Whether the memo at `root` read, directly or through the memos it read, something that changed after
    /// `revision`: an input written after it, or a memo whose value is not what it was. A memo whose value has not
    /// changed since is not walked into, whatever it read (it was verified and found the same), and one that is not
    /// there to be looked at, or has an untracked read, is taken to have read a change.
    ///
    /// The walk uses the memos' own records and executes nothing. `known` holds the answers for the memos already
    /// walked, which the dependencies of many memos share.
    fn depends_on_a_change_after(
        &self,
        root: DatabaseKeyIndex,
        revision: Revision,
        known: &mut FxHashMap<DatabaseKeyIndex, bool>,
    ) -> bool {
        use crate::zalsa_local::{QueryEdgeKind, QueryOriginRef};
        enum Look {
            Answer(bool),
            Reads(Vec<DatabaseKeyIndex>),
        }
        struct Frame {
            key: DatabaseKeyIndex,
            reads: Vec<DatabaseKeyIndex>,
            next: usize,
            found: bool,
        }
        let look = |key: DatabaseKeyIndex| -> Look {
            let ingredient = self.lookup_ingredient(key.ingredient_index());
            // An input's or a tracked struct's field changed when its record says it did; what else is not a query
            // (an interned value) is never written.
            let Some(function) = ingredient.as_function() else {
                return Look::Answer(
                    ingredient
                        .field_changed_at(self, key.key_index())
                        .is_some_and(|changed| changed > revision),
                );
            };
            // SAFETY: the caller has exclusive access to the storage: no query runs and nothing borrows a memo.
            let Some(memo) = (unsafe { function.memo_unchecked(self, key.key_index()) }) else {
                return Look::Answer(true);
            };
            if memo.header().closure().changed <= revision {
                return Look::Answer(false);
            }
            match memo.header().origin() {
                QueryOriginRef::Derived(edges) => {
                    let mut reads = Vec::new();
                    for edge in edges.iter() {
                        if edge.is_set() {
                            for member in crate::flatten::shared_set(edge.set_number()).edges() {
                                if member.kind() == QueryEdgeKind::Input && !member.is_watermark() {
                                    reads.push(member.key());
                                }
                            }
                        } else if edge.kind() == QueryEdgeKind::Input && !edge.is_watermark() {
                            reads.push(edge.key());
                        }
                    }
                    Look::Reads(reads)
                }
                // An input field written after `revision`, a value assigned by another query, or an untracked read.
                _ => Look::Answer(true),
            }
        };
        // Look at `key`: its answer if it is known at once, and otherwise a frame for it, which makes the answer
        // one that is open (a cycle that comes back to it reads nothing that changed).
        let enter = |key: DatabaseKeyIndex,
                     stack: &mut Vec<Frame>,
                     known: &mut FxHashMap<DatabaseKeyIndex, bool>|
         -> Option<bool> {
            if let Some(found) = known.get(&key) {
                return Some(*found);
            }
            match look(key) {
                Look::Answer(found) => {
                    known.insert(key, found);
                    Some(found)
                }
                Look::Reads(reads) => {
                    known.insert(key, false);
                    stack.push(Frame { key, reads, next: 0, found: false });
                    None
                }
            }
        };
        let mut stack: Vec<Frame> = Vec::new();
        if let Some(found) = enter(root, &mut stack, known) {
            return found;
        }
        loop {
            let Some(frame) = stack.last_mut() else {
                return false;
            };
            if frame.found || frame.next >= frame.reads.len() {
                let done = stack.pop().expect("a frame is open");
                known.insert(done.key, done.found);
                match stack.last_mut() {
                    None => return done.found,
                    Some(parent) => parent.found |= done.found,
                }
                continue;
            }
            let read = frame.reads[frame.next];
            frame.next += 1;
            if let Some(found) = enter(read, &mut stack, known) {
                if let Some(parent) = stack.last_mut() {
                    parent.found |= found;
                }
            }
        }
    }

    fn forget_planned(&mut self, planned: Vec<(DatabaseKeyIndex, MemoIngredientIndex)>) -> u64 {
        if planned.is_empty() {
            return 0;
        }
        // Forgetting is a change of what the database holds, like a write: the tracked structs a memo made are
        // deleted with it, and one that was read in this revision cannot be deleted in it.
        self.new_revision();
        let names: Vec<&'static str> = planned
            .iter()
            .map(|(executor, _)| self.lookup_ingredient(executor.ingredient_index()).debug_name())
            .collect();
        crate::flatten::note_forgotten(&names);
        let mut taken = Vec::new();
        for (executor, memo_index) in planned {
            // SAFETY: `&mut self` is exclusive access to the storage: no query runs and nothing
            // borrows a memo.
            let memo = unsafe {
                self.runtime
                    .table_mut()
                    .memos_mut(executor.key_index())
                    .take_memo(memo_index)
            };
            if let Some(memo) = memo {
                taken.push((executor, memo));
            }
        }
        // What a memo made goes with it, as when the query runs again without making it.
        for (executor, memo) in &taken {
            memo.remove_outputs(self, *executor);
        }
        taken.len() as u64
    }

    /// Exact keys with an already verified, final value in one of the named queries.
    /// Inspect metadata only, including for encoded values; never run or decode queries.
    #[doc(hidden)]
    pub fn cached_query_keys(&mut self, inputs: &[Id], only: &[&str]) -> Vec<Id> {
        let mut seen = FxHashSet::default();
        inputs.iter().copied().filter(|&key| {
            if !seen.insert(key) {
                return false;
            }
            let index = self.ingredient_index(key).as_u32() as usize;
            self.memo_ingredient_indices.get(index).into_iter().flatten().any(|&index| {
                let ingredient = self.lookup_ingredient(index);
                if !only.is_empty() && !only.contains(&ingredient.debug_name()) {
                    return false;
                }
                let Some(function) = ingredient.as_function() else {
                    return false;
                };
                // SAFETY: Exclusive storage access prevents queries, slot reuse and memo
                // replacement. The mapping lists only queries compatible with this key.
                unsafe { function.memo_unchecked(self, key) }
                    .is_some_and(|memo| memo.has_current_value(self.current_revision()))
            })
        }).collect()
    }

    /// Like [`Self::values_in_memory`], restricted to memos keyed directly by these inputs.
    /// An empty list selects nothing. This does not follow dependency edges or composite keys.
    #[doc(hidden)]
    pub fn values_in_memory_for_inputs(
        &mut self,
        inputs: &[Id],
        drop: bool,
        include_persisted: bool,
        only: &[&str],
        after: impl FnMut(&'static str, u64),
    ) {
        if inputs.is_empty() {
            return;
        }
        let inputs = inputs.iter().copied().collect();
        self.values_in_memory_matching(drop, include_persisted, only, Some(&inputs), after);
    }

    fn values_in_memory_matching(
        &mut self,
        drop: bool,
        include_persisted: bool,
        only: &[&str],
        inputs: Option<&FxHashSet<Id>>,
        mut after: impl FnMut(&'static str, u64),
    ) {
        for ingredient in self.ingredients() {
            if !only.is_empty() && !only.contains(&ingredient.debug_name()) {
                continue;
            }
            // SAFETY: `&mut self` is exclusive access to the storage, and the ingredient is its own.
            let held = unsafe { ingredient.values_in_memory(self, drop, include_persisted, inputs) };
            if held > 0 {
                after(ingredient.debug_name(), held);
            }
        }
    }

    /// **NOT SEMVER STABLE**
    ///
    /// Spill the values of every persisted memo to the store set with
    /// [`crate::set_spill_store`] (nothing if none is set).
    #[doc(hidden)]
    pub fn spill_persisted(&mut self) -> crate::spill::SpillReport {
        self.spill_persisted_matching(None)
    }

    /// Like [`Self::spill_persisted`], restricted to memos keyed directly by these inputs.
    /// An empty list selects nothing. This does not follow dependency edges or composite keys.
    #[doc(hidden)]
    pub fn spill_persisted_for_inputs(&mut self, inputs: &[Id]) -> crate::spill::SpillReport {
        if inputs.is_empty() {
            return crate::spill::SpillReport::default();
        }
        let inputs = inputs.iter().copied().collect();
        self.spill_persisted_matching(Some(&inputs))
    }

    fn spill_persisted_matching(
        &mut self,
        inputs: Option<&FxHashSet<Id>>,
    ) -> crate::spill::SpillReport {
        let Some(store) = crate::spill::spill_store() else {
            return crate::spill::SpillReport::default();
        };
        let counters = crate::spill::SpillCounters::default();
        for ingredient in self.ingredients() {
            // SAFETY: `&mut self` is exclusive access to the storage, and the ingredient is its own.
            unsafe { ingredient.spill_persisted(self, &*store, &counters, inputs) };
        }
        store.flush();
        counters.report()
    }

    /// **NOT SEMVER STABLE**
    #[doc(hidden)]
    pub fn evict_lru(&mut self) {
        #[cfg(feature = "detailed-trace")]
        let _span = crate::tracing::debug_span!("evict_lru").entered();
        for ingredient in &self.ingredients_requiring_reset {
            self.ingredients_vec[ingredient.as_u32() as usize]
                .reset_for_new_revision(self.runtime.table_mut());
        }
    }

    #[inline]
    pub fn ingredient_index(&self, id: Id) -> IngredientIndex {
        self.table().ingredient_index(id)
    }

    #[inline(always)]
    pub fn event(&self, event: &dyn Fn() -> crate::Event) {
        if self.event_callback.is_some() {
            self.event_cold(event);
        }
    }

    // Avoid inlining, as events are typically only enabled for debugging purposes.
    #[cold]
    #[inline(never)]
    pub fn event_cold(&self, event: &dyn Fn() -> crate::Event) {
        let event_callback = self.event_callback.as_ref().unwrap();
        event_callback(event());
    }
}

/// A type-erased `Jar`, used for ingredient registration.
#[derive(Clone, Copy)]
pub struct ErasedJar {
    kind: JarKind,
    type_id: fn() -> TypeId,
    type_name: fn() -> &'static str,
    id_struct_type_id: fn() -> TypeId,
    create_ingredients: fn(&mut Zalsa, IngredientIndex) -> Vec<Box<dyn Ingredient>>,
}

/// The kind of an `Jar`.
///
/// Note that the ordering of the variants is important. Struct ingredients must be
/// initialized before tracked functions, as tracked function ingredients depend on
/// their input struct.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
pub enum JarKind {
    /// An input/tracked/interned struct.
    Struct,

    /// A tracked function.
    TrackedFn,
}

impl ErasedJar {
    /// Performs type-erasure of a given ingredient.
    pub const fn erase<I: HasJar>() -> Self {
        Self {
            kind: I::KIND,
            // This is a false positive of the lint on beta, fixed on nightly.
            // FIXME: Remove this when nightly stabilizes.
            #[allow(clippy::incompatible_msrv)]
            type_id: TypeId::of::<I::Jar>,
            type_name: std::any::type_name::<I::Jar>,
            create_ingredients: <I::Jar>::create_ingredients,
            id_struct_type_id: <I::Jar>::id_struct_type_id,
        }
    }

    pub fn type_name(&self) -> &'static str {
        (self.type_name)()
    }
}

/// A salsa ingredient that can be registered in the database.
///
/// This trait is implemented for tracked functions and salsa structs.
pub trait HasJar {
    /// The [`Jar`] associated with this ingredient.
    type Jar: Jar;

    /// The [`JarKind`] for `Self::Jar`.
    const KIND: JarKind;
}

// Collect jars statically at compile-time if supported.
#[cfg(feature = "inventory")]
inventory::collect!(ErasedJar);

#[cfg(feature = "inventory")]
pub use inventory::submit as register_jar;

#[cfg(not(feature = "inventory"))]
#[macro_export]
#[doc(hidden)]
macro_rules! register_jar {
    ($($_:tt)*) => {};
}

#[cfg(not(feature = "inventory"))]
pub use crate::register_jar;

/// Given a wide pointer `T`, extracts the data pointer (typed as `U`).
///
/// # Safety
///
/// `U` must be correct type for the data pointer.
pub unsafe fn transmute_data_ptr<T: ?Sized, U>(t: &T) -> &U {
    let t: *const T = t;
    let u: *const U = t as *const U;
    // SAFETY: the caller must guarantee that `T` is a wide pointer for `U`
    unsafe { &*u }
}

/// Given a wide pointer `T`, extracts the data pointer (typed as `U`).
///
/// # Safety
///
/// `U` must be correct type for the data pointer.
pub(crate) unsafe fn transmute_data_mut_ptr<T: ?Sized, U>(t: &mut T) -> &mut U {
    let t: *mut T = t;
    let u: *mut U = t as *mut U;
    // SAFETY: the caller must guarantee that `T` is a wide pointer for `U`
    unsafe { &mut *u }
}
