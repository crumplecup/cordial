//! What each kind of cache holds, found by dropping it.
//!
//! The process's memory says how much is held and not by what. A query's memos can be dropped (each
//! is computed again when something reads it, so dropping is what an eviction does), and what the
//! process gives back is what that query held. Done for every query in turn, at the moment the
//! memory is highest, it gives a ledger: the bytes each cache held, and, at the end, the bytes
//! nothing dropped could give back, which is the least the process holds with the caches it has.
//!
//! Each query is measured after the ones before it were dropped, so memory that two queries share is
//! credited to the first. The meter is the process's resident memory, which is read where the
//! allocator gives memory back; small objects freed into the allocator's arenas may not show.

use std::time::{Duration, Instant};

use tracing::instrument;

/// What dropping one query's values gave back.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct LedgerRow {
    /// The query.
    query: String,
    /// How many of its memos held a value in memory.
    #[getter(copy)]
    memos: u64,
    /// What the process gave back, in kibibytes (negative if it grew while waiting).
    #[getter(copy)]
    freed_kib: i64,
    /// How long the memory took to settle, in milliseconds.
    #[getter(copy)]
    settle_ms: u64,
    /// The thread that dropped the query's values, named for the row: what a profile that counts
    /// the bytes each thread frees (`hotpath-alloc`) says of that thread is what the drop freed.
    thread: String,
}

/// The whole ledger: each query's row, and what the process held before and after.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct Ledger {
    rows: Vec<LedgerRow>,
    /// Resident memory before anything was dropped, in kibibytes.
    #[getter(copy)]
    before_kib: u64,
    /// Resident memory after every query was dropped: what the caches could not give back.
    #[getter(copy)]
    floor_kib: u64,
}

/// How long to wait for memory that is being given back, at most, and how little a poll must change
/// for it to have settled.
const SETTLE_AT_MOST: Duration = Duration::from_millis(1_500);
const SETTLE_POLL: Duration = Duration::from_millis(20);
const SETTLED_WITHIN_KIB: u64 = 512;

/// The process's resident memory once it has stopped falling, and how long that took.
///
/// Some of what is dropped is given back by another thread a little later (the syntax crate drops
/// trees in batches), so one reading right after a drop would miss it.
#[instrument(level = "trace")]
pub(crate) fn settled() -> Option<(u64, u64)> {
    let started = Instant::now();
    let mut last = crate::process_memory::sample()?.resident_kib();
    loop {
        std::thread::sleep(SETTLE_POLL);
        let now = crate::process_memory::sample()?.resident_kib();
        let calm = last.abs_diff(now) <= SETTLED_WITHIN_KIB;
        last = now;
        if calm || started.elapsed() >= SETTLE_AT_MOST {
            let waited = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            return Some((now, waited));
        }
    }
}

/// Drop the values of each query of `db` in turn (those `only` names, or all if it is empty), and
/// say what the process gave back for each. The queries are computed again when something reads
/// what was dropped.
///
/// Each query is dropped on a thread of its own, named `lg000`, `lg001`, ... in the order of the rows.
/// A profile that counts the bytes every thread frees (`hotpath-alloc`) then has, for each of those
/// threads, exactly the bytes that query's values were, whatever the allocator does with them; the
/// process's resident memory, which is also read, may not move for small objects.
#[instrument(level = "debug", skip(db))]
#[hotpath::measure]
pub fn itemize(db: &mut dyn salsa::Database, only: &[&str]) -> Option<Ledger> {
    let (before, _) = settled()?;
    let mut queries: Vec<(&'static str, u64)> = Vec::new();
    db.values_in_memory(false, true, only, &mut |query, memos| {
        queries.push((query, memos));
    });
    let mut last = before;
    let mut rows = Vec::new();
    for (index, (query, memos)) in queries.into_iter().enumerate() {
        let thread = format!("lg{index:03}");
        std::thread::scope(|scope| {
            let dropper = std::thread::Builder::new()
                .name(thread.clone())
                .spawn_scoped(scope, || {
                    // Counted from this thread's first allocation, which is what gives it a name in
                    // a profile.
                    let _registered = Box::new(0_u8);
                    db.values_in_memory(true, true, &[query], &mut |_, _| {});
                });
            match dropper {
                Ok(handle) => {
                    // A panic in the drop is the drop's, and the ledger goes on with the next.
                    let _ = handle.join();
                }
                Err(error) => {
                    tracing::warn!(error = ?error, "could not start the thread that drops a query");
                }
            }
        });
        let Some((now, waited)) = settled() else {
            continue;
        };
        let freed =
            i64::try_from(last).unwrap_or(i64::MAX) - i64::try_from(now).unwrap_or(i64::MAX);
        rows.push(LedgerRow::new(
            query.to_owned(),
            memos,
            freed,
            waited,
            thread,
        ));
        last = now;
    }
    Some(Ledger::new(rows, before, last))
}
