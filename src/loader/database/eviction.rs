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

/// The queries whose values are scaffolding: built to compute what is kept, and made again when read.
const SCAFFOLDING: [&str; 5] = [
    "EditionedFileId::parse_",
    "MacroCallId::parse_macro_expansion_",
    "HirFileId::ast_id_map_",
    "MacroCallId::macro_arg_",
    "real_span_map",
];

/// How many persisted queries had executed at the last chunk boundary, and how many chunks have
/// computed something.
static LAST_EXECUTIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static COMPUTING_CHUNKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many stages this process has finished.
static STAGES_THIS_PROCESS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// When the costly actions are done between the chunks of a pass (see [`crate::Backoff`]).
static CHUNK_BACKOFF: std::sync::Mutex<crate::Backoff> =
    std::sync::Mutex::new(crate::Backoff::new());

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
        self.jettison_scaffolding(after);
        self.evict_quietly(after);
        self.spill_under_pressure(after);
        self.drop_regenerable_if_still_pressed(after);
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
        let after_sample = crate::process_memory::sample();
        let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        tracing::info!(
            target: "cordial::eviction",
            step = after,
            micros,
            dropped_regenerable = dropped,
            rounds = 1_u64,
            resident_before_kib = before.map_or(0, |sample| sample.resident_kib()),
            resident_after_kib = after_sample.map_or(0, |sample| sample.resident_kib()),
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
    pub(super) fn evict_between_chunks(&mut self, stores_results: bool) -> bool {
        let pressed = crate::memory_limit::pressured();
        self.jettison_scaffolding("prime chunk");
        if self.evicts() {
            self.evict_quietly("prime chunk");
        }
        if pressed
            && CHUNK_BACKOFF
                .lock()
                .is_ok_and(|mut backoff| backoff.act_now())
        {
            // The costly actions: when a round of them gave back nothing (the allocator may keep
            // what is freed, and the resident memory then does not fall) the next waits twice as
            // long, so that a run is not made to compute the same values again on every chunk.
            let before = crate::memory_limit::meter_kib().unwrap_or(0);
            self.spill_under_pressure("prime chunk");
            if crate::memory_limit::pressure() >= crate::Pressure::Hard {
                // From the second rung, what is not persisted is dropped after each chunk,
                // whatever it costs the next to build it again.
                self.drop_regenerable("prime chunk");
            }
            let after = crate::memory_limit::meter_kib().unwrap_or(0);
            if let Ok(mut backoff) = CHUNK_BACKOFF.lock() {
                backoff.gave_back(before.saturating_sub(after));
            }
        }
        if self.checkpoint_wanted(stores_results) {
            self.checkpoint();
        }
        pressed
    }

    /// Whether the run should save and exit now: the watchdog saw the resident memory reach the
    /// checkpoint line, or (for a test) enough chunks that computed something have run. Only a run
    /// that a driver takes up again, and that has somewhere to save to.
    fn checkpoint_wanted(&self, stores_results: bool) -> bool {
        // Only a run that hands its results on in a store ends its processes between chunks: what a
        // process did is in the store, and not in a database that cannot be handed on.
        if !crate::memory_limit::resumable() || !crate::result_store::active() {
            return false;
        }
        // And only where what a pass computes is kept in the store: another process would do the rest
        // again, and be ended again.
        if !stores_results {
            return false;
        }
        let executed = salsa::persisted_executions();
        let before = LAST_EXECUTIONS.swap(executed, std::sync::atomic::Ordering::Relaxed);
        let computed = if executed > before {
            COMPUTING_CHUNKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
        } else {
            COMPUTING_CHUNKS.load(std::sync::atomic::Ordering::Relaxed)
        };
        let after: u64 = std::env::var(crate::CHECKPOINT_AFTER_CHUNKS_VARIABLE)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let by_memory = crate::checkpoint_requested();
        let by_count = after > 0 && computed >= after;
        if by_memory || by_count {
            tracing::info!(
                target: "cordial::eviction",
                by_memory,
                by_count,
                computing_chunks = computed,
                persisted_executions = executed,
                resident_kib = crate::memory_sample().map_or(0, |memory| memory.resident_kib()),
                held_kib = crate::memory_limit::meter_kib().unwrap_or(0),
                "a checkpoint is wanted at this chunk boundary"
            );
        }
        by_memory || by_count
    }

    /// A place between stages where a process of a run in processes may end: what the next needs is in
    /// the store already. It ends if the resident memory has reached the line, or (for a test) if it
    /// has done as many stages as the environment says.
    #[instrument(level = "debug", skip(self))]
    pub(super) fn stage_boundary(&self) {
        if !crate::result_store::active() || !crate::memory_limit::resumable() {
            return;
        }
        let done = STAGES_THIS_PROCESS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let after: u64 = std::env::var(crate::CHECKPOINT_AFTER_STAGES_VARIABLE)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let by_memory = crate::checkpoint_requested();
        let by_count = after > 0 && done >= after;
        if !(by_memory || by_count) {
            return;
        }
        let Some(store) = crate::result_store::current() else {
            return;
        };
        store.flush();
        crate::report_progress(crate::Progress::Results(store.count()));
        tracing::info!(
            target: "cordial::eviction",
            by_memory,
            by_count,
            stages_done = done,
            results = store.count(),
            resident_kib = crate::memory_sample().map_or(0, |memory| memory.resident_kib()),
            "a stage is done and this process exits, to be followed by another"
        );
        crate::memory_limit::resume_exit();
    }

    /// Save the state and exit with the status that says it is to be taken up again, unless nothing
    /// has been computed since the state this run started from: a new process would start where
    /// this one is, and not get further.
    fn checkpoint(&mut self) {
        // The results are in the store, as each was computed: what the next process needs is there,
        // and the database, with the numbers that only this process knows, is not handed on. Nothing
        // to save, and so nothing to ask of the memory that is short.
        let Some(store) = crate::result_store::current() else {
            return;
        };
        store.flush();
        crate::report_progress(crate::Progress::Working {
            results: store.count(),
            remaining: crate::memory_limit::remaining(),
            floor_kib: crate::memory_limit::floor_kib(),
        });
        tracing::info!(
            target: "cordial::eviction",
            results = store.count(),
            remaining = ?crate::memory_limit::remaining(),
            resident_kib = crate::memory_sample().map_or(0, |memory| memory.resident_kib()),
            "the results are in the store and this process exits, to be followed by another"
        );
        crate::memory_limit::resume_exit();
    }

    /// Open the existing run-cache spill store, preserving persisted values if it is unavailable.
    fn begin_spilling(&mut self) -> bool {
        if self.spill.is_some() {
            return true;
        }
        let Some(saved) = self.saved.as_ref() else {
            return false;
        };
        match super::spill::Spilling::begin(&saved.spill_path()) {
            Ok(spilling) => {
                self.spill = Some(spilling);
                true
            }
            Err(error) => {
                tracing::warn!(error = ?error, "could not open the file for spilled values");
                false
            }
        }
    }

    /// Retire values keyed by completed context crates, declarations, expanders and tracked outputs.
    /// Other composite keys and locations without current owner records still need selection rules.
    #[instrument(level = "debug", skip(self, seeds))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn retire_context_values(&mut self, seeds: &[salsa::Id], after: &str) {
        if seeds.is_empty() {
            return;
        }
        let started = std::time::Instant::now();
        let before = crate::process_memory::sample();
        let keys = salsa::Database::tracked_output_keys(&mut self.db, seeds);
        let spilling = self.begin_spilling();
        let mut selected_regenerable = 0_u64;
        salsa::Database::values_in_memory_for_inputs(
            &mut self.db,
            &keys,
            true,
            false,
            &[],
            &mut |_, held| selected_regenerable += held,
        );
        let report = if spilling {
            salsa::Database::spill_persisted_for_inputs(&mut self.db, &keys)
        } else {
            salsa::SpillReport::default()
        };
        tracing::info!(
            target: "cordial::eviction",
            step = after,
            micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            seeds = seeds.len(),
            keys = keys.len(),
            selected_regenerable,
            spilling,
            spilled = report.written,
            spilled_bytes = report.bytes,
            dropped_again = report.dropped_again,
            resident_before_kib = before.map_or(0, |sample| sample.resident_kib()),
            resident_after_kib = crate::process_memory::sample().map_or(0, |sample| sample.resident_kib()),
            "retired values keyed by completed context crates, declarations and tracked outputs"
        );
    }

    /// If the run is under memory pressure, give the values of persisted memos to the spill file,
    /// which Salsa reads them back from when a query asks for the memo. Needs the database
    /// exclusively, like an eviction, and is for the same moments. Does nothing without a run
    /// cache to put the file in.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn spill_under_pressure(&mut self, after: &str) {
        if !crate::memory_limit::pressured() || !self.begin_spilling() {
            return;
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

    /// Let go of the scaffolding the analysis builds and does not keep: the syntax trees, the results
    /// of macro expansions with their span maps, the maps of syntax ids, and macro arguments. They are
    /// built to compute what is kept (item trees, definition maps, evidence) and are not read again
    /// until something asks for that file or call, when they are made again. Under pressure they are
    /// a large share of what is held (about two thirds of a process that has just loaded a workspace),
    /// so under pressure, or once the process holds a share of its budget it should not hold more of,
    /// they go; with memory to spare they stay, as a second parse is time that
    /// memory would not have cost. Returns how many values were dropped.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn jettison_scaffolding(&mut self, after: &str) -> u64 {
        if !crate::memory_limit::scaffolding_to_go() {
            return 0;
        }
        let held_before = crate::memory_limit::meter_kib().unwrap_or(0);
        let started = std::time::Instant::now();
        let mut dropped = 0_u64;
        salsa::Database::values_in_memory(
            &mut self.db,
            true,
            false,
            &SCAFFOLDING,
            &mut |_, held| {
                dropped += held;
            },
        );
        tracing::info!(
            target: "cordial::eviction",
            after,
            micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
            dropped_scaffolding = dropped,
            held_before_kib = held_before,
            held_after_kib = crate::memory_limit::meter_kib().unwrap_or(0),
            "let go of the scaffolding: trees, expansions, span maps and id maps"
        );
        dropped
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
