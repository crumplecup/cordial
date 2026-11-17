//! Evicting what a query can compute again, between the phases of a run.
//!
//! Rust-analyzer marks its syntax trees, macro expansions and the like with a capacity, so that
//! the least recently used are dropped (their dependencies kept, to be computed again when
//! something asks). Salsa enforces those capacities only when a revision starts or when it is
//! asked to, and a run is one revision: without asking, every tree it ever parsed stays in
//! memory.

use tracing::instrument;

use super::SourceDatabase;

/// How many of the queries holding the most a report says.
const HEAP_ROWS: usize = 25;

/// The environment variable that asks a run to evict between its phases.
pub const EVICT_VARIABLE: &str = "CORDIAL_EVICT_LRU";

impl SourceDatabase {
    /// Evict what is over its capacity, if asked to, and say how long it took and how much of
    /// the process's memory it freed. Needs the database exclusively, so it is for between
    /// phases, when no query is running.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn evict_regenerable(&mut self, after: &str) {
        self.report_heap(after);
        if std::env::var_os(EVICT_VARIABLE).is_none() {
            return;
        }
        self.evict_now(after);
    }

    /// What each query holds in memory now, if the program's allocator says: the bytes by the
    /// ingredient that allocated them, most first.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn held_by_ingredient(&self) -> Vec<(String, i64)> {
        salsa::held_by_ingredient(&self.db)
    }

    /// Say what the queries hold at this point of the run, the largest few, as events.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn report_heap(&self, after: &str) {
        if let Some(total) = salsa::held_in_all() {
            tracing::info!(
                target: "cordial::heap",
                ingredient = format!("{after} total"),
                bytes = u64::try_from(total).unwrap_or(0),
                "held by the program"
            );
        }
        for (name, bytes) in self.held_by_ingredient().into_iter().take(HEAP_ROWS) {
            tracing::info!(
                target: "cordial::heap",
                ingredient = format!("{after} {name}"),
                bytes = u64::try_from(bytes).unwrap_or(0),
                "held by the queries that allocated it"
            );
        }
    }

    /// Evict what is over its capacity now, whatever the environment says. For the tests of
    /// what eviction frees. Returns the bytes the calling thread freed, which are the process's
    /// when the program counts them (and 0 when it does not).
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn evict_regenerable_now(&mut self) -> u64 {
        self.evict_now("asked")
    }

    fn evict_now(&mut self, after: &str) -> u64 {
        let before = crate::process_memory::sample();
        let held = salsa::allocated_net_bytes();
        let started = std::time::Instant::now();
        salsa::Database::trigger_lru_eviction(&mut self.db);
        let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let freed = held
            .zip(salsa::allocated_net_bytes())
            .map_or(0, |(held, now)| u64::try_from(held - now).unwrap_or(0));
        let resident = |sample: Option<crate::process_memory::MemorySample>| {
            sample.map_or(0, |sample| sample.resident_kib())
        };
        tracing::info!(
            target: "cordial::eviction",
            step = after,
            micros,
            freed_bytes = freed,
            resident_before_kib = resident(before),
            resident_after_kib = resident(crate::process_memory::sample()),
            "evicted what is over its capacity"
        );
        freed
    }
}
