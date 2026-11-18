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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpillReport {
    /// Values written to the store and dropped from memory.
    pub written: u64,
    /// The bytes written.
    pub bytes: u64,
    /// Values that had been read back and were dropped again: the store already held them.
    pub dropped_again: u64,
    /// What each query wrote: its name, how many values, and their bytes, the largest first.
    pub by_query: Vec<SpilledQuery>,
}

/// What one query wrote in a spill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpilledQuery {
    /// The query's name.
    pub query: &'static str,
    /// How many values.
    pub values: u64,
    /// Their encoded bytes.
    pub bytes: u64,
}

/// A spill's counts, added to by the ingredients (which are visited one at a time).
#[doc(hidden)]
#[derive(Default)]
pub struct SpillCounters {
    pub(crate) written: AtomicU64,
    pub(crate) bytes: AtomicU64,
    pub(crate) dropped_again: AtomicU64,
    by_query: std::sync::Mutex<std::collections::BTreeMap<&'static str, (u64, u64)>>,
}

impl SpillCounters {
    /// Count a value of `query` written, `bytes` long.
    pub(crate) fn wrote(&self, query: &'static str, bytes: u64) {
        self.written.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        if let Ok(mut by_query) = self.by_query.lock() {
            let entry = by_query.entry(query).or_default();
            entry.0 += 1;
            entry.1 += bytes;
        }
    }

    pub(crate) fn report(&self) -> SpillReport {
        let mut by_query: Vec<SpilledQuery> = self
            .by_query
            .lock()
            .map(|by_query| {
                by_query
                    .iter()
                    .map(|(query, (values, bytes))| SpilledQuery {
                        query,
                        values: *values,
                        bytes: *bytes,
                    })
                    .collect()
            })
            .unwrap_or_default();
        by_query.sort_by_key(|spilled| std::cmp::Reverse(spilled.bytes));
        SpillReport {
            written: self.written.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            dropped_again: self.dropped_again.load(Ordering::Relaxed),
            by_query,
        }
    }
}
