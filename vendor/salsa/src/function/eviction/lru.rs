//! Least Recently Used (LRU) eviction policy.
//!
//! This policy tracks the most recently accessed items and evicts
//! the least recently used ones when the cache exceeds its capacity.

use std::num::NonZeroUsize;

use crate::Id;
use crate::hash::FxLinkedHashSet;
use crate::sync::Mutex;

use super::{EvictionPolicy, HasCapacity};

/// The most any LRU keeps, when the program asks for fewer than the queries are marked with
/// (0: what they are marked with). Counted by values and not by bytes, so a program whose values
/// are large asks for fewer.
static CAPACITY_CAP: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Keep at most `cap` values of each query that has a capacity (0 to keep what each is marked
/// with). Takes effect at the next eviction.
pub fn cap_capacities(cap: usize) {
    CAPACITY_CAP.store(cap, std::sync::atomic::Ordering::Relaxed);
}

/// Whether no LRU evicts anything, whatever its mark. Each still records what it sees, so that the caps take
/// effect on the order of use when they are asked for again.
static LIFTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Keep every value of every query that has a capacity (or go back to keeping what each is marked with). For a
/// program that is not an editor: it has no reason to drop a value while it has the memory, and each value it drops
/// and then needs is computed again. Takes effect at the next eviction.
pub fn lift_capacities(lifted: bool) {
    LIFTED.store(lifted, std::sync::atomic::Ordering::Relaxed);
}

/// Least Recently Used eviction policy.
///
/// When the number of memoized values exceeds the configured capacity,
/// the least recently accessed values are evicted at the start of each
/// new revision.
pub struct Lru {
    capacity: Option<NonZeroUsize>,
    set: Mutex<FxLinkedHashSet<Id>>,
}

impl Lru {
    #[inline(never)]
    fn insert(&self, id: Id) {
        self.set.lock().insert(id);
    }
}

impl EvictionPolicy for Lru {
    fn new(cap: usize) -> Self {
        Self {
            capacity: NonZeroUsize::new(cap),
            set: Mutex::default(),
        }
    }

    #[inline(always)]
    fn record_use(&self, id: Id) {
        if self.capacity.is_some() {
            self.insert(id);
        }
    }

    fn set_capacity(&mut self, capacity: usize) {
        self.capacity = NonZeroUsize::new(capacity);
        if self.capacity.is_none() {
            self.set.get_mut().clear();
        }
    }

    fn for_each_evicted(&mut self, mut cb: impl FnMut(Id)) {
        let Some(cap) = self.capacity else {
            return;
        };
        if LIFTED.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let cap = match CAPACITY_CAP.load(std::sync::atomic::Ordering::Relaxed) {
            0 => cap.get(),
            capped => cap.get().min(capped),
        };
        let set = self.set.get_mut();
        while set.len() > cap {
            if let Some(id) = set.pop_front() {
                cb(id);
            }
        }
    }
}

impl HasCapacity for Lru {}
