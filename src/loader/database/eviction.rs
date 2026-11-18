//! Evicting what a query can compute again, between the phases of a run.
//!
//! Rust-analyzer marks its syntax trees, macro expansions and the like with a capacity, so that
//! the least recently used are dropped (their dependencies kept, to be computed again when
//! something asks). Salsa enforces those capacities only when a revision starts or when it is
//! asked to, and a run is one revision: without asking, every tree it ever parsed stays in
//! memory.

use tracing::instrument;

use super::SourceDatabase;

/// The environment variable that caps how many values each query keeps of those that keep the
/// most recently used (rust-analyzer marks them for an editor: 128 files, 512 expansions).
pub const LRU_CAP_VARIABLE: &str = "CORDIAL_LRU_CAP";

/// The environment variable that asks a run to evict between its phases.
pub const EVICT_VARIABLE: &str = "CORDIAL_EVICT_LRU";

/// How many values each query that keeps the most recently used (file trees, macro expansions)
/// keeps while the run is under memory pressure: far fewer than rust-analyzer marks them for an
/// editor, since what is dropped is computed again when something asks.
const PRESSURE_LRU_CAP: usize = 16;

/// The cap the environment asks for, if it does.
fn environment_cap() -> Option<usize> {
    std::env::var(LRU_CAP_VARIABLE)
        .ok()
        .and_then(|text| text.parse::<usize>().ok())
}

/// The cap on what each of those queries keeps: what the environment says (or each query's own
/// mark, which is 0), and under memory pressure no more than [`PRESSURE_LRU_CAP`].
fn lru_cap(pressured: bool) -> usize {
    let asked = environment_cap();
    if pressured {
        asked.map_or(PRESSURE_LRU_CAP, |cap| cap.clamp(1, PRESSURE_LRU_CAP))
    } else {
        asked.unwrap_or(0)
    }
}

/// Apply the cap the environment asks for, if it does.
pub(super) fn cap_from_environment() {
    if let Some(cap) = environment_cap() {
        salsa::cap_lru_capacities(cap);
    }
}

impl SourceDatabase {
    /// Evict what is over its capacity, if asked to, and say how long it took and how much the
    /// process's resident memory fell. Needs the database exclusively, so it is for between
    /// phases, when no query is running.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn evict_regenerable(&mut self, after: &str) {
        if !self.evicts() {
            return;
        }
        self.evict_quietly(after);
        self.spill_under_pressure(after);
    }

    /// Whether this run evicts between its stages: as its options say, or as the environment asks
    /// for an experiment.
    fn evicts(&self) -> bool {
        crate::memory_limit::pressured()
            || self
                .cargo_options
                .as_ref()
                .is_some_and(|options| *options.evicts_between_chunks())
            || std::env::var_os(EVICT_VARIABLE).is_some()
    }

    /// Evict between the chunks of a pass if asked to, or if the process is nearing its memory
    /// limit: the events of every chunk add up. Says whether it was the memory that made it, so
    /// that the pass takes smaller chunks while it lasts.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn evict_between_chunks(&mut self) -> bool {
        let pressed = crate::memory_limit::pressured();
        if self.evicts() {
            self.evict_quietly("prime chunk");
        }
        self.spill_under_pressure("prime chunk");
        pressed
    }

    /// If the run is under memory pressure, give the values of persisted memos to the spill file,
    /// which Salsa reads them back from when a query asks for the memo. Needs the database
    /// exclusively, like an eviction, and is for the same moments. Does nothing without a run
    /// cache to put the file in.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn spill_under_pressure(&mut self, after: &str) {
        if !crate::memory_limit::pressured() {
            return;
        }
        if self.spill.is_none() {
            let Some(saved) = self.saved.as_ref() else {
                return;
            };
            match super::spill::Spilling::begin(&saved.spill_path()) {
                Ok(spilling) => self.spill = Some(spilling),
                Err(error) => {
                    tracing::warn!(error = ?error, "could not open the file for spilled values");
                    return;
                }
            }
        }
        let before = crate::process_memory::sample();
        let started = std::time::Instant::now();
        let report = salsa::Database::spill_persisted(&mut self.db);
        let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let resident = |sample: Option<crate::process_memory::MemorySample>| {
            sample.map_or(0, |sample| sample.resident_kib())
        };
        for spilled in report.by_query.iter().take(8) {
            tracing::info!(
                target: "cordial::spill",
                query = spilled.query,
                step = after,
                values = spilled.values,
                bytes = spilled.bytes,
                "what a spill wrote of a query"
            );
        }
        tracing::info!(
            target: "cordial::eviction",
            step = after,
            micros,
            spilled = report.written,
            spilled_bytes = report.bytes,
            dropped_again = report.dropped_again,
            resident_before_kib = resident(before),
            resident_after_kib = resident(crate::process_memory::sample()),
            "spilled the values of persisted memos"
        );
    }

    fn evict_quietly(&mut self, after: &str) {
        let cap = lru_cap(crate::memory_limit::pressured());
        salsa::cap_lru_capacities(cap);
        let before = crate::process_memory::sample();
        let started = std::time::Instant::now();
        salsa::set_eviction_label(after);
        salsa::Database::trigger_lru_eviction(&mut self.db);
        salsa::set_eviction_label("");
        let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let resident = |sample: Option<crate::process_memory::MemorySample>| {
            sample.map_or(0, |sample| sample.resident_kib())
        };
        tracing::info!(
            target: "cordial::eviction",
            step = after,
            micros,
            cap,
            resident_before_kib = resident(before),
            resident_after_kib = resident(crate::process_memory::sample()),
            "evicted what is over its capacity"
        );
    }
}
