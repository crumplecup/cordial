use std::any::TypeId;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::cycle::{
    CycleHeads, CycleHeadsIterator, IterationStamp, ProvisionalStatus, empty_cycle_heads,
};
use crate::function::{ClaimResult, Configuration, IngredientImpl, Reentrancy};
use crate::key::DatabaseKeyIndex;
use crate::revision::AtomicRevision;
use crate::sync::atomic::Ordering;
use crate::table::memo::{DummyMemo, MemoSlot, MemoTableWithTypesMut, ToDynMemo};
use crate::zalsa::{MemoIngredientIndex, Zalsa};
use crate::zalsa_local::{QueryOriginRef, QueryRevisions};
use crate::{Cancelled, Event, EventKind, Id, Revision};

impl<C: Configuration> IngredientImpl<C> {
    /// Inserts the memo for the given key; (atomically) overwrites and returns any previously existing memo
    pub(super) fn insert_memo_into_table_for(
        &self,
        zalsa: &Zalsa,
        id: Id,
        memo: NonNull<Memo<C>>,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> Option<NonNull<Memo<C>>> {
        self.touched.touch();
        // SAFETY: The caller owns the allocation being published. Claims serialize
        // publication for this exact function/key, including provisional replacements.
        let new = unsafe { memo.as_ref() };
        let old = zalsa
            .memo_table_for::<C::SalsaStruct<'_>>(id)
            .insert(memo_ingredient_index, memo);
        if let Some(old) = old {
            // SAFETY: Replaced memos stay alive in DeletedEntries until exclusive access.
            if let Some(ownership) = unsafe { old.as_ref() }.header.revisions.ownership() {
                ownership.supersede();
            }
        }
        if new.has_ram_payload() {
            if let Some(ownership) = new.header.revisions.ownership() {
                ownership.attach(&zalsa.owner_index, self.database_key_index(id));
            }
        }
        old
    }

    /// Loads the current memo for `key_index`. This does not hold any sort of
    /// lock on the `memo_map` once it returns, so this memo could immediately
    /// become outdated if other threads store into the `memo_map`.
    pub(super) fn get_memo_from_table_for<'db>(
        &self,
        zalsa: &'db Zalsa,
        id: Id,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> Option<&'db Memo<C>> {
        let memo = zalsa
            .memo_table_for::<C::SalsaStruct<'_>>(id)
            .get(memo_ingredient_index)?;
        // SAFETY: The memo table owns this allocation for at least `'db`.
        Some(unsafe { memo.as_ref() })
    }

    pub(super) fn memo_slot<'db>(
        &self,
        zalsa: &'db Zalsa,
        id: Id,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> MemoSlot<'db> {
        // SAFETY: The table stores 'static memos (to support `Any`), but the memos remain valid
        // for `'db` because dropping them is delayed until the end of the revision.
        unsafe {
            MemoSlot::new(
                zalsa.memo_table_for::<C::SalsaStruct<'_>>(id),
                memo_ingredient_index,
            )
        }
    }

    /// Evicts the existing memo for the given key, replacing it
    /// with an equivalent memo that has no value. If the memo is untracked
    /// or has values assigned as output of another query, this has no effect.
    pub(super) fn evict_value_from_memo_for(
        table: MemoTableWithTypesMut<'_>,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> Eviction {
        let mut outcome = Eviction::Refused;
        let map = |memo: &mut Memo<C>| {
            if memo.header.can_evict_value() && !memo.header.may_be_provisional() {
                // Set the memo value to `None`.
                let spilled = memo.spilled.take().is_some();
                outcome = if memo.value.take().is_some() || spilled {
                    Eviction::Dropped
                } else {
                    Eviction::NoValue
                };
            }
            memo.detach_if_no_ram_payload();
        };

        table.map_memo(memo_ingredient_index, map);
        outcome
    }
}

/// What asking for a memo's value to be dropped did.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Eviction {
    /// The value was there and is gone.
    Dropped,
    /// The memo had no value to drop.
    NoValue,
    /// The memo's origin is not one whose value can be computed again.
    Refused,
}

/// A memoized query result.
///
/// # Layout
///
/// [`ErasedMemo`] retains a pointer to the base address of the `Memo` allocation, with spatial
/// provenance covering the entire allocation, so that it can recover the typed memo. Placing
/// `header` at offset zero also lets it access [`MemoHeader`] with a direct pointer cast. The C
/// representation makes that offset stable.
#[repr(C)]
#[derive(Debug)]
pub struct Memo<C: Configuration> {
    /// Configuration-independent state used to validate and manage this memo.
    ///
    /// Must be at offset zero for [`ErasedMemo::header`].
    pub(super) header: MemoHeader,

    /// The result of the query, if we decide to memoize it.
    pub(super) value: Option<C::Output<'static>>,

    /// Where the value is kept, when it was spilled to the store (see [`crate::spill`]): the memo
    /// then has no `value` but is valid as it was, and its value is read back when asked for.
    pub(super) spilled: Option<Box<Spilled<C>>>,
}

/// A value that is not in memory as a value: it is encoded, in the store or held here, and the copy
/// of it that was decoded, if any.
#[derive(Debug)]
pub(super) struct Spilled<C: Configuration> {
    /// Whether this is the memo a restored state held, as it was restored: the value as the bytes it was
    /// saved as. A memo that is replaced is another, and one whose value is dropped or spilled has another
    /// `Spilled` or none (see [`crate::chunks`]).
    pub(super) from_state: bool,
    pub(super) at: SpilledAt,
    pub(super) restored: std::sync::OnceLock<C::Output<'static>>,
}

/// Where an encoded value is.
#[derive(Debug)]
pub(super) enum SpilledAt {
    /// In the spill store, under this key.
    Stored(crate::spill::SpillKey),
    /// Held here as the bytes a saved state had it as, not decoded until something asks: a restored
    /// memo is valid and its value is not known until a query reads it.
    Held(Box<[u8]>),
}

/// A shared, type-erased handle to a [`Memo`].
///
/// `data` points to the base address of the `Memo<C>` allocation with spatial provenance
/// covering the entire allocation, which remains valid for shared access for `'db`, even after
/// replacement. `to_dyn_fn` and `type_id` describe the same `C`.
#[derive(Clone, Copy)]
pub(crate) struct ErasedMemo<'db> {
    /// A pointer to the base address of the [`Memo`] allocation, with spatial provenance covering
    /// the entire allocation.
    data: NonNull<DummyMemo>,

    /// Coerces `data` to a trait object using the vtable for its concrete memo type.
    to_dyn_fn: ToDynMemo,

    /// The concrete memo type, used to assert that downcasts match the registered memo type.
    type_id: TypeId,

    /// Binds shared access to the allocation lifetime.
    _lifetime: PhantomData<&'db ()>,
}

impl<'memo> ErasedMemo<'memo> {
    /// Constructs an erased handle from a memo allocation pointer and its type metadata.
    ///
    /// # Safety
    ///
    /// `data` must point to the base address of a live, aligned `Memo<C>` allocation, have
    /// spatial provenance covering the entire allocation, and remain valid for shared access for
    /// `'memo`. `to_dyn_fn` must be the trait-object coercion for `Memo<C>`, and `type_id` must be
    /// `TypeId::of::<Memo<C>>()` for that same `C`.
    #[inline]
    pub(crate) unsafe fn from_raw_parts(
        data: NonNull<DummyMemo>,
        to_dyn_fn: ToDynMemo,
        type_id: TypeId,
    ) -> Self {
        Self {
            data,
            to_dyn_fn,
            type_id,
            _lifetime: PhantomData,
        }
    }

    /// Returns the configuration-independent header without a table lookup.
    #[inline(always)]
    pub(crate) fn header(self) -> &'memo MemoHeader {
        // SAFETY: `data` points to the base address of a `Memo` allocation valid for `'memo`, with
        // spatial provenance covering the allocation, and `Memo::header` has offset zero.
        unsafe { self.data.cast::<MemoHeader>().as_ref() }
    }

    /// Returns whether the memo currently contains a value.
    #[inline]
    pub(super) fn has_value(self) -> bool {
        // SAFETY: `to_dyn_fn` matches the concrete memo allocation, which is valid for shared
        // access for `'memo`.
        unsafe { (self.to_dyn_fn)(self.data).as_ref() }.has_value()
    }

    /// A final value that can be read now without verifying dependencies.
    pub(crate) fn has_current_value(self, revision: Revision) -> bool {
        self.header().verified_at.load() == revision
            && !self.header().may_be_provisional()
            && self.has_value()
    }

    /// Returns the concrete memo after asserting that it uses configuration `C`.
    ///
    /// # Panics
    ///
    /// Panics if the memo was created for a different configuration, matching
    /// [`MemoTableWithTypes::get`](crate::table::memo::MemoTableWithTypes::get).
    #[inline]
    pub(super) fn downcast<C: Configuration>(self) -> &'memo Memo<C> {
        assert_eq!(
            self.type_id,
            TypeId::of::<Memo<C>>(),
            "ErasedMemo downcast with the wrong configuration",
        );

        // SAFETY: The type check proves that `data` points to `Memo<C>`; the handle guarantees
        // that the allocation is valid for shared access for `'memo`.
        unsafe { self.data.cast::<Memo<C>>().as_ref() }
    }
}

#[derive(Debug)]
pub(crate) struct MemoHeader {
    /// Last revision when this memo was verified; this begins
    /// as the current revision.
    pub(super) verified_at: AtomicRevision,

    /// Revision information
    pub(super) revisions: QueryRevisions,
}

impl MemoHeader {
    fn new(revision_now: Revision, revisions: QueryRevisions) -> Self {
        debug_assert!(
            !revisions.verified_final.load(Ordering::Relaxed) || revisions.cycle_heads().is_empty(),
            "Memo must be finalized if it has no cycle heads"
        );
        Self {
            verified_at: AtomicRevision::from(revision_now),
            revisions,
        }
    }

    #[inline]
    pub(crate) fn origin(&self) -> QueryOriginRef<'_> {
        self.revisions.origin()
    }

    /// Tracked identities created by this query, retained independently of its value.
    pub(crate) fn tracked_output_ids(&self) -> impl Iterator<Item = Id> + '_ {
        self.revisions.tracked_struct_ids().iter().map(|&(_, id)| id)
    }

    /// What this memo's own query says about a closure that passes through it: when it last
    /// changed, and whether it is a plain derived one (see [`crate::flatten::Closure`]).
    pub(crate) fn closure(&self) -> crate::flatten::Closure {
        crate::flatten::Closure {
            changed: self.revisions.changed_at,
            trusted: matches!(self.origin(), QueryOriginRef::Derived(_)),
        }
    }

    /// Say the memo is valid at the current revision, which something other than walking what it
    /// reads has shown (see [`crate::flatten::Closure`]).
    pub(crate) fn mark_verified_now(&self, zalsa: &Zalsa) {
        self.verified_at.store(zalsa.current_revision());
    }

    pub(super) fn can_evict_value(&self) -> bool {
        // Careful: Cannot evict memos whose values were
        // assigned as output of another query
        // or those with untracked inputs
        // as their values cannot be reconstructed. Nor one that is provisional, an iteration of a
        // cycle that is still running: with no value it is read as a cycle head that panicked, and
        // the query that reads it unwinds (an LRU that evicts often, as under memory pressure, meets
        // this).
        matches!(self.origin(), QueryOriginRef::Derived(_)) && !self.may_be_provisional()
    }

    /// True if this may be a provisional cycle-iteration result.
    #[inline]
    pub(super) fn may_be_provisional(&self) -> bool {
        // Relaxed is OK here, because `verified_final` is only ever mutated in one direction (from
        // `false` to `true`), and changing it to `true` on memos with cycle heads where it was
        // ever `false` is purely an optimization; if we read an out-of-date `false`, it just means
        // we might go validate it again unnecessarily.
        !self.revisions.verified_final.load(Ordering::Relaxed)
    }

    /// Cycle heads that should be propagated to dependent queries.
    #[inline(always)]
    pub(super) fn cycle_heads(&self) -> &CycleHeads {
        if self.may_be_provisional() {
            self.revisions.cycle_heads()
        } else {
            empty_cycle_heads()
        }
    }

    /// Returns `true` if this memo was part of a cycle in it's last iteration.
    #[inline(always)]
    pub(super) fn was_cycle_participant(&self) -> bool {
        !self.revisions.cycle_heads().is_empty()
    }

    /// Mark memo as having been verified in the `revision_now`, which should
    /// be the current revision.
    /// The caller is responsible to update the memo's `accumulated` state if their accumulated
    /// values have changed since.
    #[inline]
    pub(super) fn mark_as_verified(&self, zalsa: &Zalsa, database_key_index: DatabaseKeyIndex) {
        zalsa.event(&|| {
            Event::new(EventKind::DidValidateMemoizedValue {
                database_key: database_key_index,
            })
        });

        self.verified_at.store(zalsa.current_revision());
    }

    pub(super) fn mark_outputs_as_verified(
        &self,
        zalsa: &Zalsa,
        database_key_index: DatabaseKeyIndex,
    ) {
        for output in self.revisions.origin().outputs() {
            output.mark_validated_output(zalsa, database_key_index);
        }
    }

    pub(super) fn tracing_debug(&self, has_value: bool) -> impl std::fmt::Debug + use<'_> {
        struct TracingDebug<'memo> {
            header: &'memo MemoHeader,
            has_value: bool,
        }

        impl std::fmt::Debug for TracingDebug<'_> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("Memo")
                    .field(
                        "value",
                        if self.has_value {
                            &"Some(<value>)"
                        } else {
                            &"None"
                        },
                    )
                    .field("verified_at", &self.header.verified_at)
                    .field("revisions", &self.header.revisions)
                    .finish()
            }
        }

        TracingDebug {
            header: self,
            has_value,
        }
    }

    pub(super) fn remove_outputs(&self, zalsa: &Zalsa, executor: DatabaseKeyIndex) {
        for stale_output in self.revisions.origin().outputs() {
            stale_output.remove_stale_output(zalsa, executor);
        }

        for (identity, id) in self.revisions.tracked_struct_ids() {
            let key = DatabaseKeyIndex::new(identity.ingredient_index(), *id);
            key.remove_stale_output(zalsa, executor);
        }
    }
}

impl<C: Configuration> Memo<C> {
    pub(super) fn new(
        value: Option<C::Output<'_>>,
        revision_now: Revision,
        revisions: QueryRevisions,
    ) -> Self {
        Self {
            value: value.map(|value| {
                // SAFETY: Guaranteed by `Configuration` and retained only in this memo.
                unsafe { std::mem::transmute::<C::Output<'_>, C::Output<'static>>(value) }
            }),
            header: MemoHeader::new(revision_now, revisions),
            spilled: None,
        }
    }

    /// Whether this is the memo a restored state held, as it was restored (see [`Spilled::from_state`]).
    pub(super) fn is_from_state(&self) -> bool {
        self.value.is_none()
            && self
                .spilled
                .as_ref()
                .is_some_and(|spilled| spilled.from_state && matches!(spilled.at, SpilledAt::Held(_)))
    }

    /// The value, read back from the store if it was spilled.
    pub(super) fn value(&self) -> Option<&C::Output<'_>> {
        let value = match self.value.as_ref() {
            Some(value) => value,
            None => {
                let spilled = self.spilled.as_ref()?;
                spilled.restored.get_or_init(|| restore::<C>(&spilled.at))
            }
        };
        // SAFETY: Guaranteed by `Configuration`; the restored lifetime is
        // bounded by the borrow of this memo.
        Some(unsafe { std::mem::transmute::<&C::Output<'static>, &C::Output<'_>>(value) })
    }

    pub(super) fn has_ram_payload(&self) -> bool {
        self.in_memory()
            || self
                .spilled
                .as_ref()
                .is_some_and(|spilled| matches!(spilled.at, SpilledAt::Held(_)))
    }

    /// Whether a value is in memory as a value, as computed or decoded.
    pub(super) fn in_memory(&self) -> bool {
        self.value.is_some()
            || self
                .spilled
                .as_ref()
                .is_some_and(|spilled| spilled.restored.get().is_some())
    }

    /// Drop the value in memory. A memo that has the value as bytes can decode it again; one that
    /// does not is computed again when something reads it. Needs exclusive access to the memo, and
    /// does nothing to one whose value cannot be computed again.
    pub(super) fn drop_in_memory_value(&mut self) {
        if let Some(spilled) = self.spilled.as_mut() {
            spilled.restored.take();
        } else if self.header.can_evict_value() && !self.header.may_be_provisional() {
            self.value = None;
        }
        self.detach_if_no_ram_payload();
    }

    pub(super) fn detach_if_no_ram_payload(&self) {
        if !self.has_ram_payload() {
            if let Some(ownership) = self.header.revisions.ownership() {
                ownership.detach();
            }
        }
    }

    /// Whether the memo has a value, in memory or in the store.
    pub(super) fn holds_value(&self) -> bool {
        self.value.is_some() || self.spilled.is_some()
    }

    /// Returns `true` if this memo should be serialized.
    pub(super) fn should_serialize(&self) -> bool {
        // TODO: Serialization is a good opportunity to prune old query results based on
        // the `verified_at` revision.
        self.holds_value() && !self.header.may_be_provisional()
    }

    pub(super) fn tracing_debug(&self) -> impl std::fmt::Debug + use<'_, C> {
        self.header.tracing_debug(self.holds_value())
    }

    /// Write the value to the store and drop it from memory, or, if it was read back, drop that
    /// copy (the store has it). Needs exclusive access to the memo. Nothing is done to a memo whose
    /// value cannot be computed again, or is provisional, or whose query is not persisted.
    #[cfg(feature = "persistence")]
    pub(super) fn spill(
        &mut self,
        key: crate::spill::SpillKey,
        store: &dyn crate::spill::SpillStore,
        counters: &crate::spill::SpillCounters,
    ) {
        use std::sync::atomic::Ordering::Relaxed;
        if !C::PERSIST || !self.header.can_evict_value() || self.header.may_be_provisional() {
            return;
        }
        if let Some(value) = self.value.as_ref() {
            let mut bytes = Vec::new();
            let written = C::serialize(&value, &mut rmp_serde::Serializer::new(&mut bytes));
            if written.is_err() {
                // A value that cannot be encoded stays where it is.
                return;
            }
            store.put(key, &bytes);
            counters.wrote(C::DEBUG_NAME, bytes.len() as u64);
            self.value = None;
            self.spilled = Some(Box::new(Spilled {
                from_state: false,
                at: SpilledAt::Stored(key),
                restored: std::sync::OnceLock::new(),
            }));
        } else if let Some(spilled) = self.spilled.as_mut() {
            if let SpilledAt::Held(bytes) = &spilled.at {
                // The bytes a saved state had are given to the store, and the memo keeps its key.
                store.put(key, bytes);
                counters.wrote(C::DEBUG_NAME, bytes.len() as u64);
                spilled.at = SpilledAt::Stored(key);
                spilled.restored.take();
            } else if spilled.restored.take().is_some() {
                counters.dropped_again.fetch_add(1, Relaxed);
            }
        }
        self.detach_if_no_ram_payload();
    }

    /// The value as bytes when it is not in memory as a value: what a saved state had it as, or
    /// what the store holds. `None` for a memo whose value is in memory.
    #[cfg(feature = "persistence")]
    pub(super) fn encoded(&self) -> Option<std::borrow::Cow<'_, [u8]>> {
        let spilled = self.spilled.as_ref()?;
        Some(match &spilled.at {
            SpilledAt::Held(bytes) => std::borrow::Cow::Borrowed(bytes),
            SpilledAt::Stored(key) => std::borrow::Cow::Owned(
                crate::spill::spill_store()
                    .and_then(|store| store.get(*key))
                    .unwrap_or_else(|| panic!("a spilled value is not in the store: {key:?}")),
            ),
        })
    }
}

/// Decode a value that was spilled or restored without being decoded.
fn restore<C: Configuration>(at: &SpilledAt) -> C::Output<'static> {
    #[cfg(feature = "persistence")]
    {
        let fetched;
        let bytes: &[u8] = match at {
            SpilledAt::Held(bytes) => bytes,
            SpilledAt::Stored(key) => {
                fetched = crate::spill::spill_store()
                    .and_then(|store| store.get(*key))
                    .unwrap_or_else(|| panic!("a spilled value is not in the store: {key:?}"));
                &fetched
            }
        };
        C::deserialize(&mut rmp_serde::Deserializer::new(bytes))
            .unwrap_or_else(|error| panic!("a spilled value could not be decoded: {error}"))
    }
    #[cfg(not(feature = "persistence"))]
    {
        let _ = at;
        panic!("a spilled value needs the persistence feature")
    }
}

impl<C: Configuration> crate::table::memo::Memo for Memo<C> {
    fn has_value(&self) -> bool {
        self.holds_value()
    }

    fn remove_outputs(&self, zalsa: &Zalsa, executor: DatabaseKeyIndex) {
        self.header.remove_outputs(zalsa, executor);
    }

    #[cfg(feature = "salsa_unstable")]
    fn memory_usage(&self) -> crate::database::MemoInfo {
        let size_of = std::mem::size_of::<Memo<C>>() + self.header.revisions.allocation_size();
        let heap_size = self.value().map_or(Some(0), C::heap_size);

        crate::database::MemoInfo {
            debug_name: C::DEBUG_NAME,
            output: crate::database::SlotInfo {
                size_of_metadata: size_of - std::mem::size_of::<C::Output<'static>>(),
                debug_name: std::any::type_name::<C::Output<'static>>(),
                size_of_fields: std::mem::size_of::<C::Output<'static>>(),
                heap_size_of_fields: heap_size,
                memos: Vec::new(),
            },
        }
    }
}

#[cfg(feature = "persistence")]
mod persistence {
    use crate::function::Configuration;
    use crate::function::memo::{Memo, MemoHeader};
    use crate::revision::AtomicRevision;
    use crate::zalsa_local::QueryRevisions;
    use crate::zalsa_local::persistence::{MappedQueryRevisions, PersistentQueryOrigin};

    use serde::Deserialize;
    use serde::ser::SerializeStruct;

    /// A reference to the fields of a [`Memo`], with its [`QueryRevisions`] transformed.
    pub(crate) struct MappedMemo<'memo, C: Configuration> {
        /// The value, when it is in memory as one.
        pub(crate) value: Option<&'memo C::Output<'memo>>,
        /// The value as bytes, when it is not: a restored memo nothing has read, or a spilled one.
        pub(crate) encoded: Option<std::borrow::Cow<'memo, [u8]>>,
        pub(crate) verified_at: AtomicRevision,
        pub(crate) revisions: MappedQueryRevisions<'memo>,
    }

    impl<C: Configuration> Memo<C> {
        pub(crate) fn with_origin(
            &self,
            serialized_origin: PersistentQueryOrigin,
        ) -> MappedMemo<'_, C> {
            // A value that is still bytes is saved as the bytes it is, and is not decoded to be encoded.
            let encoded = self.encoded();
            let value = if encoded.is_some() { None } else { self.value() };
            let Memo { ref header, .. } = *self;
            let MemoHeader {
                ref verified_at,
                ref revisions,
            } = *header;

            MappedMemo {
                value,
                encoded,
                verified_at: AtomicRevision::from(verified_at.load()),
                revisions: revisions.with_origin(serialized_origin),
            }
        }
    }

    impl<C> serde::Serialize for MappedMemo<'_, C>
    where
        C: Configuration,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            struct SerializeValue<'me, 'db, C: Configuration>(&'me C::Output<'db>);

            impl<C> serde::Serialize for SerializeValue<'_, '_, C>
            where
                C: Configuration,
            {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    C::serialize(self.0, serializer)
                }
            }

            struct AsBytes<'a>(&'a [u8]);

            impl serde::Serialize for AsBytes<'_> {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    serializer.serialize_bytes(self.0)
                }
            }

            let MappedMemo {
                value,
                encoded,
                verified_at,
                revisions,
            } = self;

            // The value is saved as bytes of its own, so that a restore can keep them until a query
            // asks for the value.
            let owned;
            let bytes: &[u8] = match (encoded, value) {
                (Some(encoded), _) => encoded,
                (None, Some(value)) => {
                    let mut buffer = Vec::new();
                    SerializeValue::<C>(value)
                        .serialize(&mut rmp_serde::Serializer::new(&mut buffer))
                        .map_err(serde::ser::Error::custom)?;
                    owned = buffer;
                    &owned
                }
                (None, None) => panic!(
                    "attempted to serialize memo where `Memo::should_serialize` returned `false`"
                ),
            };

            let mut s = serializer.serialize_struct("Memo", 3)?;
            s.serialize_field("value", &AsBytes(bytes))?;
            s.serialize_field("verified_at", &verified_at)?;
            s.serialize_field("revisions", &revisions)?;
            s.end()
        }
    }

    impl<'de, C> serde::Deserialize<'de> for Memo<C>
    where
        C: Configuration,
    {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            #[derive(Deserialize)]
            #[serde(rename = "Memo")]
            pub struct DeserializeMemo {
                value: EncodedValue,
                verified_at: AtomicRevision,
                revisions: QueryRevisions,
            }

            /// The value as the bytes it was saved as.
            struct EncodedValue(Vec<u8>);

            impl<'de> serde::Deserialize<'de> for EncodedValue {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    struct Bytes;

                    impl<'de> serde::de::Visitor<'de> for Bytes {
                        type Value = EncodedValue;

                        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                            f.write_str("the bytes of a memo's value")
                        }

                        fn visit_bytes<E>(self, bytes: &[u8]) -> Result<EncodedValue, E> {
                            Ok(EncodedValue(bytes.to_vec()))
                        }

                        fn visit_byte_buf<E>(self, bytes: Vec<u8>) -> Result<EncodedValue, E> {
                            Ok(EncodedValue(bytes))
                        }

                        fn visit_seq<A>(self, mut seq: A) -> Result<EncodedValue, A::Error>
                        where
                            A: serde::de::SeqAccess<'de>,
                        {
                            let mut bytes = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                            while let Some(byte) = seq.next_element::<u8>()? {
                                bytes.push(byte);
                            }
                            Ok(EncodedValue(bytes))
                        }
                    }

                    deserializer.deserialize_bytes(Bytes)
                }
            }

            let memo = DeserializeMemo::deserialize(deserializer)?;

            // The value stays as the bytes it was saved as until a query reads the memo.
            // A state saved with canonical stamps says its memos are valid as of the save, which
            // is the revision restored.
            let verified_at = crate::stamps::restored_current().unwrap_or_else(|| memo.verified_at.load());
            let mut restored = Memo::<C>::new(None, verified_at, memo.revisions);
            restored.spilled = Some(Box::new(super::Spilled {
                from_state: false,
                at: super::SpilledAt::Held(memo.value.0.into_boxed_slice()),
                restored: std::sync::OnceLock::new(),
            }));
            if let Some(spilled) = restored.spilled.as_mut() {
                spilled.from_state = true;
            }
            Ok(restored)
        }
    }
}

#[derive(Debug)]
pub(super) enum TryClaimHeadsResult {
    /// Claiming the cycle head results in a cycle.
    Cycle {
        head_iteration: IterationStamp,
        memo_iteration: IterationStamp,
        verified_at: Revision,
    },

    /// The cycle head is not finalized, but it can be claimed.
    Available,

    /// The cycle head is currently executed on another thread.
    Running,
}

/// Iterator to try claiming the transitive cycle heads of a memo.
pub(super) struct TryClaimCycleHeadsIter<'a> {
    zalsa: &'a Zalsa,

    cycle_heads: CycleHeadsIterator<'a>,
}

impl<'a> TryClaimCycleHeadsIter<'a> {
    pub(super) fn new(zalsa: &'a Zalsa, cycle_heads: &'a CycleHeads) -> Self {
        Self {
            zalsa,

            cycle_heads: cycle_heads.iter(),
        }
    }
}

impl Iterator for TryClaimCycleHeadsIter<'_> {
    type Item = TryClaimHeadsResult;

    fn next(&mut self) -> Option<Self::Item> {
        let head = self.cycle_heads.next()?;
        let head_database_key = head.database_key_index;
        let head_key_index = head_database_key.key_index();
        let ingredient = self
            .zalsa
            .lookup_ingredient(head_database_key.ingredient_index());
        let function = ingredient
            .as_function()
            .expect("cycle heads must be function ingredients");

        match function
            .sync_table()
            .peek_claim(self.zalsa, head_key_index, Reentrancy::Deny)
        {
            ClaimResult::Cycle { .. } => {
                // We hit a cycle blocking on the cycle head; this means this query actively
                // participates in the cycle and some other query is blocked on this thread.
                crate::tracing::trace!("Waiting for {head_database_key:?} results in a cycle");

                let provisional_status = function
                    .provisional_status(self.zalsa, head_key_index)
                    .expect("cycle head memo to exist");
                let (current_iteration, verified_at) = match provisional_status {
                    ProvisionalStatus::Provisional {
                        iteration,
                        verified_at,
                        cycle_heads: _,
                    } => (iteration, verified_at),
                    ProvisionalStatus::Final {
                        iteration,
                        verified_at,
                    } => (iteration, verified_at),
                    ProvisionalStatus::Poisoned {
                        iteration,
                        verified_at,
                    } => {
                        if verified_at == self.zalsa.current_revision()
                            && iteration.cancellation_count()
                                == self.zalsa.runtime().cancellation_count()
                        {
                            Cancelled::PropagatedPanic.throw();
                        }

                        return Some(TryClaimHeadsResult::Available);
                    }
                };

                Some(TryClaimHeadsResult::Cycle {
                    memo_iteration: current_iteration,
                    head_iteration: head.iteration.load(),
                    verified_at,
                })
            }
            ClaimResult::Running(running) => {
                crate::tracing::trace!("Ingredient {head_database_key:?} is running: {running:?}");

                Some(TryClaimHeadsResult::Running)
            }
            ClaimResult::Claimed(()) => Some(TryClaimHeadsResult::Available),
        }
    }
}

#[cfg(all(not(feature = "shuttle"), target_pointer_width = "64"))]
mod _memory_usage {
    use crate::cycle::CycleRecoveryStrategy;
    use crate::ingredient::Location;
    use crate::plumbing::{self, IngredientIndices, MemoIngredientSingletonIndex, SalsaStructInDb};
    use crate::table::memo::MemoTableWithTypes;
    use crate::zalsa::Zalsa;
    use crate::{Database, Id, Revision};

    use std::any::TypeId;
    use std::num::NonZeroUsize;

    // Required by `ErasedMemo::header`.
    const _: () = assert!(std::mem::offset_of!(super::Memo<DummyConfiguration>, header) == 0);
    const _: () = assert!(
        std::mem::offset_of!(super::Memo<DummyConfiguration>, value)
            == std::mem::size_of::<super::MemoHeader>()
    );

    // Memos are stored a lot, make sure their size doesn't randomly increase.
    const _: [(); std::mem::size_of::<super::MemoHeader>()] =
        [(); std::mem::size_of::<[usize; 4]>()];
    // A word more than it was: the spilled value's place (a null pointer when there is none).
    const _: [(); std::mem::size_of::<super::Memo<DummyConfiguration>>()] =
        [(); std::mem::size_of::<[usize; 6]>()];
    const _: [(); std::mem::size_of::<super::ErasedMemo<'static>>()] =
        [(); std::mem::size_of::<[usize; 4]>()];

    struct DummyStruct;

    impl SalsaStructInDb for DummyStruct {
        type MemoIngredientMap = MemoIngredientSingletonIndex;
        const LEAF_TYPE_IDS: &'static [typeid::ConstTypeId] = &[];

        fn lookup_ingredient_index(_: &Zalsa) -> IngredientIndices {
            unimplemented!()
        }

        fn cast(_: Id, _: TypeId) -> Option<Self> {
            unimplemented!()
        }

        unsafe fn memo_table(_: &Zalsa, _: Id, _: Revision) -> MemoTableWithTypes<'_> {
            unimplemented!()
        }

        fn entries(_: &Zalsa) -> impl Iterator<Item = crate::DatabaseKeyIndex> + '_ {
            std::iter::empty()
        }
    }

    struct DummyConfiguration;

    // SAFETY: `NonZeroUsize` is `'static` and contains no database lifetime.
    unsafe impl super::Configuration for DummyConfiguration {
        const DEBUG_NAME: &'static str = "";
        const LOCATION: Location = Location { file: "", line: 0 };
        const PERSIST: bool = false;
        const CYCLE_STRATEGY: CycleRecoveryStrategy = CycleRecoveryStrategy::Panic;

        type DbView = dyn Database;
        type SalsaStruct<'db> = DummyStruct;
        type Input<'db> = ();
        type Output<'db> = NonZeroUsize;
        type Eviction = crate::function::eviction::NoopEviction;

        fn values_equal<'db>(_: &Self::Output<'db>, _: &Self::Output<'db>) -> bool {
            unimplemented!()
        }

        fn id_to_input(_: &Zalsa, _: Id) -> Self::Input<'_> {
            unimplemented!()
        }

        fn execute<'db>(_: &'db Self::DbView, _: Self::Input<'db>) -> Self::Output<'db> {
            unimplemented!()
        }

        fn cycle_initial<'db>(
            _: &'db Self::DbView,
            _: Id,
            _: Self::Input<'db>,
        ) -> Self::Output<'db> {
            unimplemented!()
        }

        fn recover_from_cycle<'db>(
            _: &'db Self::DbView,
            _: &crate::Cycle,
            _: &Self::Output<'db>,
            value: Self::Output<'db>,
            _: Self::Input<'db>,
        ) -> Self::Output<'db> {
            value
        }

        fn serialize<S>(_: &Self::Output<'_>, _: S) -> Result<S::Ok, S::Error>
        where
            S: plumbing::serde::Serializer,
        {
            unimplemented!()
        }

        fn deserialize<'de, D>(_: D) -> Result<Self::Output<'static>, D::Error>
        where
            D: plumbing::serde::Deserializer<'de>,
        {
            unimplemented!()
        }
    }
}
