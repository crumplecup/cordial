//! Evicting what a query can compute again, between the phases of a run.
//!
//! Rust-analyzer marks its syntax trees, macro expansions and the like with a capacity, so that
//! the least recently used are dropped (their dependencies kept, to be computed again when
//! something asks). Salsa enforces those capacities only when a revision starts or when it is
//! asked to, and a run is one revision: without asking, every tree it ever parsed stays in
//! memory.

use tracing::instrument;

use super::SourceDatabase;

/// The environment variable that asks a run to evict between its phases.
pub const EVICT_VARIABLE: &str = "CORDIAL_EVICT_LRU";

impl SourceDatabase {
    /// Evict what is over its capacity, if asked to, and say how long it took and how much of
    /// the process's memory it freed. Needs the database exclusively, so it is for between
    /// phases, when no query is running.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn evict_regenerable(&mut self, after: &str) {
        if std::env::var_os(EVICT_VARIABLE).is_none() {
            return;
        }
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
    }
}
