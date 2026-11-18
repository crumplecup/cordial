//! Giving the values of persisted memos to a store when memory is short, and taking them back when
//! something asks.
//!
//! A memo that is spilled keeps everything that says whether it is still valid (what it read, when it
//! was verified) and loses only its value, which is written to the store. The next query that reads
//! the memo finds it valid, and its value is read back from the store and decoded, not computed again.
//! A value that was read back and is spilled again is not written again: the store still holds it.
//!
//! Only the values of persisted queries can be spilled, since they are the ones that can be encoded.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// Where a memo's value is kept in a store: the database it is of, the query, and the key it was
/// asked with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpillKey {
    /// The database's storage (databases of one process are kept apart by it).
    pub storage: u32,
    /// The query.
    pub ingredient: u32,
    /// The key the query was asked with.
    pub id: u64,
}

/// A place values are spilled to, which must give back what was put in it.
pub trait SpillStore: Send + Sync {
    /// Keep `bytes` under `key`, replacing what was there.
    fn put(&self, key: SpillKey, bytes: &[u8]);

    /// What was put under `key`.
    fn get(&self, key: SpillKey) -> Option<Vec<u8>>;

    /// Called when a spill is over, so that a store that keeps what it was given in memory to write
    /// it together can write the rest.
    fn flush(&self) {}
}

static STORE: RwLock<Option<Arc<dyn SpillStore>>> = RwLock::new(None);

/// Use `store` for the values spilled from now on (and read back), or none.
pub fn set_spill_store(store: Option<Arc<dyn SpillStore>>) {
    if let Ok(mut current) = STORE.write() {
        *current = store;
    }
}

pub(crate) fn spill_store() -> Option<Arc<dyn SpillStore>> {
    STORE.read().ok().and_then(|store| store.clone())
}

/// What a spill did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpillReport {
    /// Values written to the store and dropped from memory.
    pub written: u64,
    /// The bytes written.
    pub bytes: u64,
    /// Values that had been read back and were dropped again: the store already held them.
    pub dropped_again: u64,
}

/// A spill's counts, added to by the ingredients (which are visited one at a time).
#[derive(Default)]
#[doc(hidden)]
pub struct SpillCounters {
    pub(crate) written: AtomicU64,
    pub(crate) bytes: AtomicU64,
    pub(crate) dropped_again: AtomicU64,
}

impl SpillCounters {
    pub(crate) fn report(&self) -> SpillReport {
        SpillReport {
            written: self.written.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            dropped_again: self.dropped_again.load(Ordering::Relaxed),
        }
    }
}
