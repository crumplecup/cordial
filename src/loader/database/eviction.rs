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

/// The environment variable that caps how many values each query keeps of those that keep the
/// most recently used (rust-analyzer marks them for an editor: 128 files, 512 expansions).
pub const LRU_CAP_VARIABLE: &str = "CORDIAL_LRU_CAP";

/// The environment variable that asks a run to evict between its phases.
pub const EVICT_VARIABLE: &str = "CORDIAL_EVICT_LRU";

/// Apply the cap the environment asks for, if it does.
pub(super) fn cap_from_environment() {
    if let Some(cap) = std::env::var(LRU_CAP_VARIABLE)
        .ok()
        .and_then(|text| text.parse::<usize>().ok())
    {
        salsa::cap_lru_capacities(cap);
    }
}

/// How often the memory held is read while waiting for deferred frees, and how long to wait for it
/// to stop falling at most.
const SETTLE_EVERY: std::time::Duration = std::time::Duration::from_millis(20);
const SETTLE_AT_MOST: std::time::Duration = std::time::Duration::from_secs(5);

/// Wait until the memory the program holds has not fallen for several readings in a row (or a
/// while has passed) and give it.
fn settle_held() -> i64 {
    let began = std::time::Instant::now();
    let mut last = salsa::held_in_all().unwrap_or(0);
    let mut steady = 0;
    while steady < 6 && began.elapsed() < SETTLE_AT_MOST {
        std::thread::sleep(SETTLE_EVERY);
        let now = salsa::held_in_all().unwrap_or(0);
        steady = if now < last { 0 } else { steady + 1 };
        last = now;
    }
    last
}

impl SourceDatabase {
    /// Evict what is over its capacity, if asked to, and say how long it took and how much of
    /// the process's memory it freed. Needs the database exclusively, so it is for between
    /// phases, when no query is running.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn evict_regenerable(&mut self, after: &str) {
        self.report_heap(after);
        if !self.evicts() {
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

    /// Whether this run evicts between its stages: as its options say, or as the environment asks
    /// for an experiment.
    fn evicts(&self) -> bool {
        self.cargo_options
            .as_ref()
            .is_some_and(|options| *options.evicts_between_chunks())
            || std::env::var_os(EVICT_VARIABLE).is_some()
    }

    /// Say what the queries hold at this point of the run, the largest few, as events.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn report_heap(&self, after: &str) {
        crate::peak_heap::note_stage(after);
        if let Some(total) = salsa::held_in_all() {
            tracing::info!(
                target: "cordial::heap",
                ingredient = format!("{after} total"),
                bytes = u64::try_from(total).unwrap_or(0),
                "held by the program"
            );
        }
        if let Some(peak) = salsa::held_peak() {
            tracing::info!(
                target: "cordial::heap",
                ingredient = format!("{after} peak"),
                bytes = u64::try_from(peak).unwrap_or(0),
                "most held by the program so far"
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

    /// Evict between the chunks of a pass, if asked to: no heap report, and the events of every
    /// chunk add up.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn evict_between_chunks(&mut self) {
        if !self.evicts() {
            return;
        }
        self.evict_quietly("prime chunk");
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
        let freed = self.evict_quietly(after);
        self.report_heap(&format!("evicted {after}"));
        freed
    }

    fn evict_quietly(&mut self, after: &str) -> u64 {
        let before = crate::process_memory::sample();
        let held = salsa::allocated_net_bytes();
        let held_in_all = salsa::held_in_all();
        let started = std::time::Instant::now();
        salsa::set_eviction_label(after);
        salsa::Database::trigger_lru_eviction(&mut self.db);
        salsa::set_eviction_label("");
        let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        // A syntax tree dropped by the eviction is not freed by it: rust-analyzer hands it to a
        // thread that frees it a little later. What the eviction gives back is what is held when that
        // has run, so wait for the program's memory to stop falling, and say how long that took.
        let settle_started = std::time::Instant::now();
        let settled = held_in_all.map(|_| settle_held());
        let settled_micros =
            u64::try_from(settle_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let freed_settled = held_in_all
            .zip(settled)
            .map_or(0, |(held, now)| u64::try_from(held - now).unwrap_or(0));
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
            settled_micros,
            freed_settled_bytes = freed_settled,
            resident_before_kib = resident(before),
            resident_after_kib = resident(crate::process_memory::sample()),
            "evicted what is over its capacity"
        );
        freed
    }
}
