//! A dependency on one key of something that holds many, with the revision each key last changed in.
//!
//! A field of a tracked struct is one dependency however many things it holds: a reader of a module's scope
//! depends on the whole scope, and a name added to it makes every reader run again, the ones that looked up
//! another name included. A keyed dependency is on one key. The code that makes the thing says which keys it
//! changed ([`touch`]), and a reader says which key it read ([`read`]), found or not: a key that is not there is
//! a key like any other, and it changes when the key is added. What is stored is a revision per key that was ever
//! touched; nothing is stored for a key that never was, and nothing is stored per reader.

use std::any::TypeId;
use std::fmt;

use crate::function::VerifyResult;
use crate::hash::{FxHashMap, FxHashSet, FxIndexSet};
use crate::ingredient::{Ingredient, Jar, Location};
use crate::sync::Arc;
use crate::table::memo::MemoTableTypes;
use crate::zalsa::{HasJar, IngredientIndex, JarKind, Zalsa};
use crate::zalsa_local::QueryEdge;
use crate::{Database, DatabaseKeyIndex, Durability, Id, Revision};

/// How many keyed reads, and how many keys were touched, in this process: what a region of a run says it did.
static READS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static TOUCHES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many keyed reads have been made and how many keys touched, in this process.
pub fn counts() -> (u64, u64) {
    (
        READS.load(std::sync::atomic::Ordering::Relaxed),
        TOUCHES.load(std::sync::atomic::Ordering::Relaxed),
    )
}

/// The jar of the keyed ingredient, registered with every database.
pub(crate) struct KeyedJar;

pub(crate) struct KeyedHasJar;

impl HasJar for KeyedHasJar {
    type Jar = KeyedJar;
    const KIND: JarKind = JarKind::Struct;
}

impl Jar for KeyedJar {
    fn create_ingredients(_zalsa: &mut Zalsa, first_index: IngredientIndex) -> Vec<Box<dyn Ingredient>> {
        vec![Box::new(KeyedIngredient::new(first_index))]
    }

    fn id_struct_type_id() -> TypeId {
        TypeId::of::<KeyedIngredient>()
    }
}

/// The revision each key last changed in. A key is an [`Id`] the caller chose (an interned value's, say).
pub struct KeyedIngredient {
    index: IngredientIndex,
    changed: std::sync::RwLock<FxHashMap<Id, Revision>>,
    /// The revision every key changed in at the latest, whatever it says: set when what changed is not known.
    floor: std::sync::atomic::AtomicUsize,
    /// Whether a key was touched, or the floor raised, since the state this was restored from was restored (or
    /// ever, if it was not restored): what a save of it would hold is then not what that state holds.
    changed_since_restore: std::sync::atomic::AtomicBool,
}

/// What is saved is a map, as every ingredient is, from the bits of a key to the revision it changed in. No key has
/// the bits 0, so the floor is the entry of that.
#[cfg(feature = "persistence")]
const FLOOR_KEY: u64 = 0;

impl KeyedIngredient {
    fn new(index: IngredientIndex) -> Self {
        Self {
            index,
            changed: std::sync::RwLock::new(FxHashMap::default()),
            floor: std::sync::atomic::AtomicUsize::new(Revision::start().as_usize()),
            changed_since_restore: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn of(zalsa: &Zalsa) -> &Self {
        let index = zalsa.lookup_jar_by_type::<KeyedJar>();
        zalsa.lookup_ingredient(index).assert_type::<Self>()
    }

    /// The revision `key` last changed in: the start for a key that never was touched.
    fn changed_at(&self, key: Id) -> Revision {
        let changed = self.changed.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let touched = changed.get(&key).copied().unwrap_or_else(Revision::start);
        touched.max(Revision::from(self.floor.load(std::sync::atomic::Ordering::Acquire)))
    }

    fn raise_floor(&self, revision: Revision) {
        self.changed_since_restore.store(true, std::sync::atomic::Ordering::Release);
        self.floor.fetch_max(revision.as_usize(), std::sync::atomic::Ordering::AcqRel);
    }

    fn touch(&self, key: Id, revision: Revision) {
        self.changed_since_restore.store(true, std::sync::atomic::Ordering::Release);
        let mut changed = self.changed.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        let at = changed.entry(key).or_insert(revision);
        *at = (*at).max(revision);
    }
}

/// Say that the running query read `key`, whether it is there or not, and that what it read has the
/// `durability` of what holds it.
pub fn read(db: &dyn Database, key: Id, durability: Durability) {
    READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (zalsa, zalsa_local) = db.zalsas();
    let ingredient = KeyedIngredient::of(zalsa);
    zalsa_local.report_tracked_read_simple(
        DatabaseKeyIndex::new(ingredient.index, key),
        durability,
        ingredient.changed_at(key),
    );
}

/// Runs `read`, which only reads fields, without the running query depending on what it reads. A reader that does
/// this says what it depends on instead, with [`read`]. Nothing that runs a query belongs inside it: what such a
/// query read would not be recorded either.
///
/// It also says the lowest durability of the fields read, which is the durability of what the reader depends on.
pub fn unrecorded<R>(db: &dyn Database, read: impl FnOnce() -> R) -> (R, Durability) {
    let zalsa_local = db.zalsa_local();
    let scope = zalsa_local.unrecorded_scope();
    let read = read();
    let durability = zalsa_local.unrecorded_durability();
    drop(scope);
    (read, durability)
}

/// Say that `key` changed in the current revision: it was added, removed or given another value.
pub fn touch(db: &dyn Database, key: Id) {
    TOUCHES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let zalsa = db.zalsa();
    KeyedIngredient::of(zalsa).touch(key, zalsa.current_revision());
}

/// Say that every key changed in the current revision, because what changed is not known: there is no map to
/// say it against, as after a restore.
pub fn touch_all(db: &dyn Database) {
    let zalsa = db.zalsa();
    KeyedIngredient::of(zalsa).raise_floor(zalsa.current_revision());
}

/// The revision `key` last changed in, as a reader of it would find it.
pub fn changed_at(db: &dyn Database, key: Id) -> Revision {
    KeyedIngredient::of(db.zalsa()).changed_at(key)
}

impl Ingredient for KeyedIngredient {
    fn location(&self) -> &'static Location {
        &const { Location { file: file!(), line: line!() } }
    }

    fn ingredient_index(&self) -> IngredientIndex {
        self.index
    }

    fn field_changed_at(&self, _zalsa: &Zalsa, id: Id) -> Option<Revision> {
        Some(self.changed_at(id))
    }

    unsafe fn maybe_changed_after(
        &self,
        _zalsa: &Zalsa,
        _db: crate::database::RawDatabase<'_>,
        input: Id,
        revision: Revision,
    ) -> VerifyResult {
        VerifyResult::changed_if(self.changed_at(input) > revision)
    }

    unsafe fn collect_minimum_serialized_edges(
        &self,
        _zalsa: &Zalsa,
        edge: QueryEdge,
        serialized_edges: &mut FxIndexSet<QueryEdge>,
        _flattener: &mut crate::flatten::EdgeFlattener,
    ) {
        serialized_edges.insert(edge);
    }

    fn flatten_cycle_head_dependencies(
        &self,
        _zalsa: &Zalsa,
        id: Id,
        flattened_input_outputs: &mut FxIndexSet<QueryEdge>,
        _seen: &mut FxHashSet<DatabaseKeyIndex>,
    ) {
        flattened_input_outputs.insert(QueryEdge::input(DatabaseKeyIndex::new(self.index, id)));
    }

    fn fmt_index(&self, index: Id, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "keyed({index:?})")
    }

    fn debug_name(&self) -> &'static str {
        "keyed"
    }

    fn is_persistable(&self) -> bool {
        true
    }

    /// A state that is saved says the revision of every key that changed, so that a memo that was out of date when
    /// it was saved is found out of date when it is restored. Nothing is saved while no key changed.
    fn should_serialize(&self, _zalsa: &Zalsa) -> bool {
        let changed = self.changed.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        !changed.is_empty() || self.floor.load(std::sync::atomic::Ordering::Acquire) > Revision::start().as_usize()
    }

    fn unchanged_since_restore(&self) -> bool {
        !self.changed_since_restore.load(std::sync::atomic::Ordering::Acquire)
    }

    fn mark_restored(&self) {
        self.changed_since_restore.store(false, std::sync::atomic::Ordering::Release);
    }

    #[cfg(feature = "persistence")]
    unsafe fn serialize<'db>(&'db self, _zalsa: &'db Zalsa, f: &mut dyn FnMut(&dyn erased_serde::Serialize)) {
        // Sorted by key, so that a state saved again is the same whatever order the table is in.
        let mut saved: std::collections::BTreeMap<u64, Revision> = {
            let changed = self.changed.read().unwrap_or_else(|poisoned| poisoned.into_inner());
            changed.iter().map(|(key, revision)| (key.as_bits(), *revision)).collect()
        };
        saved.insert(FLOOR_KEY, Revision::from(self.floor.load(std::sync::atomic::Ordering::Acquire)));
        f(&saved);
    }

    #[cfg(feature = "persistence")]
    fn deserialize(
        &mut self,
        _zalsa: &mut Zalsa,
        deserializer: &mut dyn erased_serde::Deserializer,
    ) -> Result<(), erased_serde::Error> {
        let saved: std::collections::BTreeMap<u64, Revision> = erased_serde::deserialize(deserializer)?;
        let changed = self.changed.get_mut().unwrap_or_else(|poisoned| poisoned.into_inner());
        changed.clear();
        for (bits, revision) in saved {
            if bits == FLOOR_KEY {
                *self.floor.get_mut() = revision.as_usize();
            } else {
                changed.insert(Id::from_bits(bits), revision);
            }
        }
        Ok(())
    }

    fn jar_kind(&self) -> JarKind {
        JarKind::Struct
    }

    fn memo_table_types(&self) -> &Arc<MemoTableTypes> {
        unreachable!("the keyed ingredient does not allocate pages")
    }

    fn memo_table_types_mut(&mut self) -> &mut Arc<MemoTableTypes> {
        unreachable!("the keyed ingredient does not allocate pages")
    }
}

impl fmt::Debug for KeyedIngredient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyedIngredient").field("index", &self.index).finish()
    }
}
