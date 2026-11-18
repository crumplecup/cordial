//! Evicting what a query can compute again, between the phases of a run.
//!
//! Rust-analyzer marks its syntax trees, macro expansions and the like with a capacity, so that
//! the least recently used are dropped (their dependencies kept, to be computed again when
//! something asks). Salsa enforces those capacities only when a revision starts or when it is
//! asked to, and a run is one revision: without asking, every tree it ever parsed stays in
//! memory.

use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use tracing::instrument;

use super::SourceDatabase;

/// The environment variable that caps how many values each query keeps of those that keep the
/// most recently used (rust-analyzer marks them for an editor: 128 files, 512 expansions).
pub const LRU_CAP_VARIABLE: &str = "CORDIAL_LRU_CAP";

/// The environment variable that asks a run for a ledger of what each query holds.
pub const LEDGER_VARIABLE: &str = "CORDIAL_MEMORY_LEDGER";

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
        self.drop_regenerable_if_still_pressed(after);
    }

    /// If the run asks for it, drop the values of each query in turn and say what the process gave
    /// back for each, with what else the process holds that no query's value is.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn itemize_memory(&mut self, after: &str) {
        let asked = self
            .cargo_options
            .as_ref()
            .is_some_and(|options| *options.memory_ledger())
            || std::env::var_os(LEDGER_VARIABLE).is_some();
        if !asked {
            return;
        }
        let texts: (usize, usize) = self
            .vfs
            .iter()
            .map(|(file, _)| self.db.file_text(file).text(&self.db).len())
            .fold((0, 0), |(files, bytes), len| (files + 1, bytes + len));
        let threads = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status
                    .lines()
                    .find_map(|line| line.strip_prefix("Threads:"))
                    .and_then(|rest| rest.trim().parse::<u64>().ok())
            })
            .unwrap_or(0);
        let Some(ledger) = crate::itemize(&mut self.db, &[]) else {
            return;
        };
        for row in ledger.rows() {
            tracing::info!(
                target: "cordial::ledger",
                ingredient = row.query().as_str(),
                thread = row.thread().as_str(),
                step = after,
                memos = row.memos(),
                freed_kib = row.freed_kib().max(0).unsigned_abs(),
                settle_ms = row.settle_ms(),
                "what dropping a query's values gave back"
            );
        }
        tracing::info!(
            target: "cordial::ledger",
            step = after,
            before_kib = ledger.before_kib(),
            floor_kib = ledger.floor_kib(),
            file_texts = texts.0 as u64,
            file_text_kib = (texts.1 / 1024) as u64,
            threads,
            "what a run held before and after every query's values were dropped"
        );
    }

    /// If the run is still nearing its memory limit after the eviction and the spill, drop the
    /// values of every query that is not persisted: they are computed again when something reads
    /// them, and a saved state does not hold them. For the ends of stages, and not between the
    /// chunks of a pass, where the next chunk would build them all again.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn drop_regenerable_if_still_pressed(&mut self, after: &str) {
        if !crate::memory_limit::nearing_now() {
            return;
        }
        self.drop_regenerable(after);
    }

    /// Drop the values of every query that is not persisted, and say what it gave back.
    fn drop_regenerable(&mut self, after: &str) {
        let before = crate::process_memory::sample();
        let started = std::time::Instant::now();
        let dropped = salsa::Database::drop_regenerable_values(&mut self.db);
        let settled = crate::memory_ledger::settled();
        let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        tracing::info!(
            target: "cordial::eviction",
            step = after,
            micros,
            dropped_regenerable = dropped,
            resident_before_kib = before.map_or(0, |sample| sample.resident_kib()),
            resident_after_kib = settled.map_or(0, |(resident, _)| resident),
            "dropped the values of every query that is not persisted"
        );
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
        if crate::memory_limit::pressure() >= crate::Pressure::Hard {
            // From the second rung, what is not persisted is dropped after each chunk, whatever it
            // costs the next to build it again.
            self.drop_regenerable("prime chunk");
        }
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
        if crate::memory_limit::pressured() {
            let (files, bytes) = self.drop_library_texts();
            tracing::info!(
                target: "cordial::eviction",
                after,
                library_texts = files,
                library_text_bytes = bytes,
                "the texts of library files left memory"
            );
        }
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
