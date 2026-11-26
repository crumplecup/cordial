//! A bound on how much memory a run may use, enforced by the process itself.
//!
//! Analysing a large workspace takes many gigabytes, and a run that grows without bound can
//! exhaust the machine before anything notices. A watchdog thread reads the process's resident
//! memory (see [`crate::process_memory`]) and, when it passes the limit, says where the run was
//! and exits, so that the limit is a decision the person made and not whatever the system does
//! when it runs out.
//!
//! The limit is a size (`12G`, `512M`) or a share of the memory available when the run starts
//! (`75%`). "Available" is the kernel's own estimate of what a new program could use without
//! the system swapping (`MemAvailable`): it already leaves out what the person's other
//! programs hold, so a share of it keeps a buffer for the machine.

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::Duration;

use tracing::instrument;

/// The limit when none is given: three quarters of the memory available at the start.
pub const DEFAULT_MEMORY_LIMIT: &str = "75%";

/// What the limit is when the memory available cannot be read (a platform without
/// `/proc/meminfo`): a size that is safe on a small machine.
const FALLBACK_BYTES: u64 = 8 << 30;

/// The exit status of a run that stopped itself for using too much memory.
pub const MEMORY_LIMIT_EXIT: i32 = 75;

/// The exit status of a run that saved its state to be taken up again, because its resident memory
/// neared the budget: the driver that started it starts the next.
pub const MEMORY_RESUME_EXIT: i32 = 76;

/// Set (to anything) by the driver that restarts a run that exits with [`MEMORY_RESUME_EXIT`]. A
/// run without it never exits to be taken up again: nothing would take it up.
pub const RESUMABLE_VARIABLE: &str = "CORDIAL_RESUMABLE";

/// For a test: the run saves and exits, to be taken up again, after this many chunks that computed
/// something, whatever its memory.
pub const CHECKPOINT_AFTER_CHUNKS_VARIABLE: &str = "CORDIAL_CHECKPOINT_AFTER_CHUNKS";

/// For a test: a process of a run in processes ends after this many stages (the ordinary projection,
/// each cfg context), whatever its memory.
pub const CHECKPOINT_AFTER_STAGES_VARIABLE: &str = "CORDIAL_CHECKPOINT_AFTER_STAGES";

/// The share of the budget, in hundredths, that the resident memory reaches before a run that can be
/// taken up again saves its state and exits, so that the next process starts with nothing the
/// allocator kept.
pub const CHECKPOINT_PERCENT: u64 = 85;

/// Whether this run is started by a driver that takes it up again.
#[must_use]
pub fn resumable() -> bool {
    std::env::var_os(RESUMABLE_VARIABLE).is_some()
}

/// Whether a process using `resident_kib` has reached [`CHECKPOINT_PERCENT`] of `ladder_bytes`.
/// A budget of zero is none.
#[must_use]
pub fn checkpoint_due(resident_kib: u64, ladder_bytes: u64) -> bool {
    ladder_bytes != 0
        && resident_kib.saturating_mul(1024) >= ladder_bytes / 100 * CHECKPOINT_PERCENT
}

/// How parsimonious a run is: the share of its budget a process may hold, with no pressure, before
/// the scaffolding the analysis builds (trees, expansions, span maps) is let go of. Pressure begins
/// at sixty per cent of the budget, which is too late for what is built in one step, so a run that
/// is parsimonious lets go earlier. Below the line memory is abundant for the work, and a second
/// parse is time that memory would not have cost. `0` is none: only pressure lets go of it.
pub const DEFAULT_PARSIMONY_PERCENT: u64 = 30;

/// The names of the settings of parsimony, and the share of the budget each is: from the least
/// room kept for scaffolding (`lean`, which suits a machine with little to spare, at the cost of
/// parsing again what is wanted again) to the most (`roomy`); `off` leaves it to pressure.
pub const PARSIMONY_NAMES: [(&str, u64); 4] = [
    ("lean", 15),
    ("balanced", DEFAULT_PARSIMONY_PERCENT),
    ("roomy", 50),
    ("off", 0),
];

static PARSIMONY_PERCENT: AtomicU64 = AtomicU64::new(DEFAULT_PARSIMONY_PERCENT);

/// Read a setting of parsimony: a name from [`PARSIMONY_NAMES`], or a share of the budget from 1 to
/// 100 (with or without a `%`), or `0` for none.
///
/// # Errors
///
/// Says what was wrong with `text` and what is accepted.
pub fn parse_parsimony(text: &str) -> Result<u64, String> {
    let wanted = text.trim().to_ascii_lowercase();
    if let Some((_, percent)) = PARSIMONY_NAMES.iter().find(|(name, _)| *name == wanted) {
        return Ok(*percent);
    }
    match wanted.trim_end_matches('%').parse::<u64>() {
        Ok(percent) if percent <= 100 => Ok(percent),
        _ => Err(format!(
            "`{text}` is not a setting of parsimony: use lean, balanced, roomy, off, or a share of the budget from 0 to 100"
        )),
    }
}

/// The setting of parsimony: the flag, else `cordial.toml`, else the default.
#[must_use]
pub fn resolve_parsimony(flag: Option<u64>, configured: Option<&str>) -> u64 {
    flag.or_else(|| {
        configured.and_then(|text| match parse_parsimony(text) {
            Ok(percent) => Some(percent),
            Err(reason) => {
                let _ = writeln!(
                    crate::host::stderr(),
                    "cordial.toml: memory_parsimony: {reason}; using the default"
                );
                None
            }
        })
    })
    .unwrap_or(DEFAULT_PARSIMONY_PERCENT)
}

/// Set how parsimonious the run is, as a share of the budget (see [`DEFAULT_PARSIMONY_PERCENT`]).
pub fn be_parsimonious_at(percent: u64) {
    PARSIMONY_PERCENT.store(percent, Ordering::Relaxed);
}

/// Whether a process holding `held_kib` of a budget of `ladder_bytes` holds enough that, at
/// `percent` of it, the scaffolding should be let go of. No budget and no share are no line, and
/// nothing is let go of.
#[must_use]
pub fn scaffolding_due(held_kib: u64, ladder_bytes: u64, percent: u64) -> bool {
    ladder_bytes != 0
        && percent != 0
        && held_kib.saturating_mul(1024) >= ladder_bytes / 100 * percent
}

/// Say what budget this run works to, as an event its metrics keep: the line at which it lets go of
/// the scaffolding it builds is a share of the budget, and a run that never lets go of any should
/// show whether it was ever given a line.
#[instrument(level = "debug")]
pub fn log_budget() {
    let ladder = ladder_bytes();
    let percent = PARSIMONY_PERCENT.load(Ordering::Relaxed);
    tracing::info!(
        target: "cordial::budget",
        ladder_kib = ladder / 1024,
        parsimony_percent = percent,
        scaffolding_line_kib = if ladder == 0 || percent == 0 { 0 } else { ladder / 100 * percent / 1024 },
        available_kib = crate::process_memory::available_kib().unwrap_or(0),
        "the memory this run works to, and the line at which it lets go of scaffolding"
    );
}

/// Whether the scaffolding should be let go of now: the run feels pressure, or holds a share of its
/// budget that it should not hold more of.
#[must_use]
pub fn scaffolding_to_go() -> bool {
    pressured()
        || meter_kib().is_some_and(|held| {
            scaffolding_due(
                held,
                ladder_bytes(),
                PARSIMONY_PERCENT.load(Ordering::Relaxed),
            )
        })
}

/// How much more than a process holds the machine sees of it once the allocator has kept what the
/// program freed, as a share of what is held: what a process needs to be given besides what it holds.
/// Measured at 18 to 50 per cent on the elicit_doc runs; this is near the low end, since a process
/// that lets go of what it does not need and allocates again reuses much of what was kept.
pub const ALLOCATOR_OVERHEAD_PERCENT: u64 = 25;

/// The room a process has left before it ends, as a share of the line, below which it is taken to
/// have none: one that has loaded the workspace and has less than this to work in does a chunk of
/// work and ends again, and so does every process after it.
pub const ROOM_PERCENT: u64 = 10;

/// The least room, in bytes, a process is taken to have: a process with less does too little between
/// the load that comes first and the end that comes after.
pub const ROOM_LEAST_BYTES: u64 = 64 << 20;

/// The memory, in bytes, at which a process of a run with `ladder_bytes` ends: the checkpoint line.
#[must_use]
pub fn checkpoint_line_bytes(ladder_bytes: u64) -> u64 {
    ladder_bytes / 100 * CHECKPOINT_PERCENT
}

/// Whether a process that holds `floor_kib` once the workspace is loaded has no room to work in
/// before the checkpoint line of a run with `ladder_bytes`: no process of the run can do anything
/// but the load, and the run is not one that processes can finish within its budget. A budget of
/// zero is none, and has room.
#[must_use]
pub fn no_room(floor_kib: u64, ladder_bytes: u64) -> bool {
    if ladder_bytes == 0 {
        return false;
    }
    let line = checkpoint_line_bytes(ladder_bytes);
    let least = (line / 100 * ROOM_PERCENT).max(ROOM_LEAST_BYTES);
    line.saturating_sub(floor_kib.saturating_mul(1024)) < least
}

/// The budget, in bytes, that a process with `floor_kib` needs to have room: its checkpoint line (a
/// share of the budget) has to be the floor and the room it is taken to need, which is the larger of
/// [`ROOM_PERCENT`] of the line and [`ROOM_LEAST_BYTES`]. Rounded up to a mebibyte, and a mebibyte
/// more, for the whole numbers the line is made of.
#[must_use]
pub fn budget_with_room(floor_kib: u64) -> u64 {
    let floor = floor_kib.saturating_mul(1024);
    // The line that leaves ROOM_PERCENT of it, and the line that leaves the least room.
    let by_share = floor.saturating_mul(100) / (100 - ROOM_PERCENT);
    let by_least = floor.saturating_add(ROOM_LEAST_BYTES);
    let line = by_share.max(by_least);
    let budget = line.saturating_mul(100).div_ceil(CHECKPOINT_PERCENT.max(1));
    (budget.div_ceil(1 << 20) + 1) << 20
}

/// How much work of the pass that keeps its results in the store is left, as the last chunk left it
/// (`u64::MAX` when no such pass has run): what a process that ends says of the run, so that the
/// driver can tell how many more processes it needs at the rate they go at.
static REMAINING: AtomicU64 = AtomicU64::new(u64::MAX);

/// What a process held once it had loaded the workspace and listed its functions, in kibibytes.
static FLOOR_KIB: AtomicU64 = AtomicU64::new(0);

/// Say how much of the pass that keeps its results in the store is left.
pub fn note_remaining(items: u64) {
    REMAINING.store(items, Ordering::Relaxed);
}

/// How much of the pass that keeps its results in the store is left, if it has begun.
#[must_use]
pub fn remaining() -> Option<u64> {
    let items = REMAINING.load(Ordering::Relaxed);
    (items != u64::MAX).then_some(items)
}

/// Say what this process held once it had loaded the workspace and listed its functions.
pub fn note_floor(floor_kib: u64) {
    FLOOR_KIB.store(floor_kib, Ordering::Relaxed);
}

/// What this process held once it had loaded the workspace and listed its functions, in kibibytes.
#[must_use]
pub fn floor_kib() -> u64 {
    FLOOR_KIB.load(Ordering::Relaxed)
}

/// The memory, in bytes, the run's processes work to: the target if there is one, else the limit.
#[must_use]
pub fn ladder_bytes() -> u64 {
    LIMIT_BYTES.load(Ordering::Relaxed)
}

/// Say that no process of the run has room to work in, and exit: the driver says what the budget
/// would have to be.
pub fn no_room_exit(floor_kib: u64) -> ! {
    let status = crate::LiveStatus::global();
    let line_kib = checkpoint_line_bytes(LIMIT_BYTES.load(Ordering::Relaxed)) / 1024;
    crate::report_progress(crate::Progress::NoRoom {
        floor_kib,
        line_kib,
    });
    crate::flush_current();
    log_footprint("no room to work in");
    crate::run_metrics::write_ended("memory no room", &status.phase());
    crate::profiling::finish(crate::profiling::FinishReason::NoRoom);
    std::process::exit(crate::MEMORY_STUCK_EXIT)
}

/// Whether the watchdog saw the resident memory reach the checkpoint line.
static CHECKPOINT: AtomicBool = AtomicBool::new(false);

/// Whether the run should save its state and exit at its next safe point.
#[must_use]
pub fn checkpoint_requested() -> bool {
    CHECKPOINT.load(Ordering::Relaxed)
}

/// Ask for a checkpoint at the next safe point (for a test; the watchdog does it by the memory).
pub fn request_checkpoint() {
    CHECKPOINT.store(true, Ordering::Relaxed);
}

/// Say that the run saved its state to be taken up again, and exit.
pub fn resume_exit() -> ! {
    let status = crate::LiveStatus::global();
    let _ = writeln!(
        crate::host::stderr(),
        "cordial: saved what it did, at {}, for a new process to take the run up again",
        status.phase()
    );
    crate::flush_current();
    log_footprint("ended to be followed by another process");
    crate::run_metrics::write_ended("memory checkpoint", &status.phase());
    crate::profiling::finish(crate::profiling::FinishReason::Resume);
    std::process::exit(MEMORY_RESUME_EXIT)
}

/// The most the run held, the most the machine saw of it, and what it held when the machine saw the
/// most: the watchdog's samples, kept to say at the end where the memory was.
static PEAK_HELD_KIB: AtomicU64 = AtomicU64::new(0);
static PEAK_RESIDENT_KIB: AtomicU64 = AtomicU64::new(0);
static HELD_AT_PEAK_RESIDENT_KIB: AtomicU64 = AtomicU64::new(0);

/// What a process's memory was, from its peaks: how much of what the machine saw of it was held,
/// and how much was not (what an allocator kept after the program freed it, and its own overhead).
/// The held bytes are the program's to lower; the rest is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    /// The most the process held, in kibibytes.
    pub peak_held_kib: u64,
    /// The most the machine saw of it, in kibibytes.
    pub peak_resident_kib: u64,
    /// What the machine saw at its most that the process did not hold, in kibibytes.
    pub kept_by_allocator_kib: u64,
    /// The share, in hundredths, of what the machine saw at its most that the process held.
    pub held_percent: u64,
}

/// The footprint of a process whose most resident memory was `peak_resident_kib`, of which it held
/// `held_at_peak_kib`, and whose most held was `peak_held_kib`.
#[must_use]
pub fn footprint_of(
    peak_held_kib: u64,
    peak_resident_kib: u64,
    held_at_peak_kib: u64,
) -> Footprint {
    let kept = peak_resident_kib.saturating_sub(held_at_peak_kib);
    let share = held_at_peak_kib
        .saturating_mul(100)
        .checked_div(peak_resident_kib)
        .unwrap_or(0);
    Footprint {
        peak_held_kib,
        peak_resident_kib,
        kept_by_allocator_kib: kept,
        held_percent: share,
    }
}

/// Say, as an event, what this process's memory was at its most.
pub fn log_footprint(why: &str) {
    let footprint = footprint_of(
        PEAK_HELD_KIB.load(Ordering::Relaxed),
        PEAK_RESIDENT_KIB.load(Ordering::Relaxed),
        HELD_AT_PEAK_RESIDENT_KIB.load(Ordering::Relaxed),
    );
    tracing::info!(
        target: "cordial::footprint",
        why,
        peak_held_kib = footprint.peak_held_kib,
        peak_resident_kib = footprint.peak_resident_kib,
        kept_by_allocator_kib = footprint.kept_by_allocator_kib,
        held_percent = footprint.held_percent,
        "what this process's memory was at its most"
    );
}

/// Says what the process's memory was at its most when it goes, whichever way it ends.
#[derive(Debug)]
pub struct FootprintOnDrop;

impl Drop for FootprintOnDrop {
    fn drop(&mut self) {
        log_footprint("ended");
    }
}

/// The limit in force for this run, in bytes (zero: none), kept so that the work can ease off
/// before the watchdog stops it.
static LIMIT_BYTES: AtomicU64 = AtomicU64::new(0);

/// Whether the run is nearing its limit, as the watchdog last judged it. Work reads this and not
/// the process's memory: it is one load, and the watchdog's reading is never more than a sample old.
static PRESSURE: AtomicBool = AtomicBool::new(false);

/// How hard the run is pressed, as a rung of the ladder (see `docs/planning/cold-cache.md`): each
/// does what the one before does, and more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Pressure {
    /// Below the first rung.
    #[default]
    None,
    /// From 60% of the limit: evict, spill, take smaller chunks.
    Soft,
    /// From 75%: a quarter of the workers, and what is not persisted is dropped after each chunk.
    Hard,
    /// From 85%: one worker and the smallest chunks.
    Severe,
}

impl Pressure {
    fn from_bits(bits: u8) -> Self {
        match bits {
            1 => Self::Soft,
            2 => Self::Hard,
            3.. => Self::Severe,
            0 => Self::None,
        }
    }
}

/// The share of the limit, in percent, from which each rung begins (Soft, Hard, Severe), and the
/// share below which a run on it leaves it: lower, so the work does not alternate around one figure.
const BEGINS_AT: [u64; 3] = [EASE_OFF_PERCENT, 75, 85];
const LEAVES_BELOW: [u64; 3] = [RELIEF_PERCENT, 60, 70];

/// The rung a process using `resident_kib` of `limit` bytes is on, given the one it was on: it climbs
/// as soon as a rung's share is reached, and comes down a rung only when it is below that rung's
/// leaving share. A limit of zero is none.
#[instrument(level = "trace")]
pub fn judge_level(was: Pressure, resident_kib: u64, limit: u64) -> Pressure {
    if limit == 0 {
        return Pressure::None;
    }
    let percent_times_limit = resident_kib.saturating_mul(1024).saturating_mul(100);
    let reached = BEGINS_AT
        .iter()
        .filter(|share| percent_times_limit >= limit.saturating_mul(**share))
        .count();
    let mut level = reached.max(was as usize);
    while level > reached
        && level > 0
        && percent_times_limit < limit.saturating_mul(LEAVES_BELOW[level - 1])
    {
        level -= 1;
    }
    Pressure::from_bits(u8::try_from(level).unwrap_or(3))
}

/// What one worker of the priming holds while it works, in kibibytes: a syntax tree, expansions and
/// an inference, at once. Taken from a run that went from the first rung to being stopped in under
/// four seconds, with eight workers, as the memory it grew by (about 260 MB) divided among them.
pub const WORKER_KIB: u64 = 32 * 1024;

/// How many workers fit in `headroom_kib` of memory below the second rung's line: as many as
/// [`WORKER_KIB`] goes into it, and at least one, since a run with none would not finish. `available`
/// is how many there could be with all the memory in the world.
#[instrument(level = "trace")]
pub fn workers_within(headroom_kib: i64, available: usize) -> usize {
    let fitting = u64::try_from(headroom_kib).unwrap_or(0) / WORKER_KIB;
    usize::try_from(fitting)
        .unwrap_or(usize::MAX)
        .clamp(1, available.max(1))
}

/// The memory left below the second rung's line, in kibibytes (negative when the run is past it),
/// or `None` when the run has no limit or its memory cannot be read.
#[instrument(level = "trace")]
pub fn headroom_kib() -> Option<i64> {
    let limit = LIMIT_BYTES.load(Ordering::Relaxed);
    if limit == 0 {
        return None;
    }
    let held = i64::try_from(meter_kib()?).ok()?;
    let line = i64::try_from(limit / 1024 * BEGINS_AT[1] / 100).ok()?;
    Some(line - held)
}

/// How little a round of the ladder's costly actions (dropping values, spilling, dropping texts) may
/// give back, in kibibytes, to count as having done nothing.
pub const FUTILE_KIB: u64 = 8 * 1024;

/// The most rounds skipped after rounds that did nothing, as a power of two (so 63 of every 64).
const LONGEST_BACKOFF: u32 = 6;

/// When to do the ladder's costly actions: not again right away after a round that gave nothing back.
///
/// The watchdog reads the resident memory, and an allocator may keep what the run frees, so a round
/// can free everything it holds and the reading not fall. The actions are then repeated on every
/// chunk to no end (a run did, one function at a time, forever). After each round that gives back
/// less than [`FUTILE_KIB`] the next one waits twice as long, to at most 63 rounds, and a round that
/// does give something back starts again.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Backoff {
    futile_in_a_row: u32,
    skip: u32,
}

impl Backoff {
    /// A backoff that has seen nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            futile_in_a_row: 0,
            skip: 0,
        }
    }

    /// Whether this round should do the costly actions: yes unless it is one to skip.
    #[instrument(level = "trace")]
    pub fn act_now(&mut self) -> bool {
        if self.skip > 0 {
            self.skip -= 1;
            return false;
        }
        true
    }

    /// Say what a round that did the actions gave back.
    #[instrument(level = "trace")]
    pub fn gave_back(&mut self, freed_kib: u64) {
        if freed_kib >= FUTILE_KIB {
            *self = Self::default();
            return;
        }
        self.futile_in_a_row = (self.futile_in_a_row + 1).min(LONGEST_BACKOFF);
        self.skip = (1u32 << self.futile_in_a_row) - 1;
    }
}

/// Whether the run is over its budget: what it holds is over the limit, or what the machine sees
/// of it is. The resident memory is the one a person's machine runs out of, and an allocator that
/// keeps what is freed can fill it while the program holds little, so it is the budget too: a run is
/// not let to take more of the machine than it was given, whatever it holds. A limit of zero is none.
#[must_use]
pub fn over_budget(held_kib: u64, resident_kib: u64, limit_bytes: u64) -> bool {
    exceeds(held_kib, limit_bytes) || exceeds(resident_kib, limit_bytes)
}

/// The memory the limit is judged by, in kibibytes: resident process memory.
/// Freed objects may leave resident pages in the allocator until it releases them.
#[must_use]
pub fn meter_of(resident_kib: u64) -> u64 {
    resident_kib
}

/// [`meter_of`] for the process as it is now.
#[must_use]
pub fn meter_kib() -> Option<u64> {
    crate::process_memory::sample().map(|memory| meter_of(memory.resident_kib()))
}

/// How many workers run on the rung the run is on: all of them, then a quarter, then one. Every
/// worker holds a tree, expansions and an inference at once, so fewer is less held at the peak.
#[must_use]
pub fn workers_at(available: usize, level: Pressure) -> usize {
    match level {
        Pressure::None | Pressure::Soft => available,
        Pressure::Hard => (available / 4).max(1),
        Pressure::Severe => 1,
    }
}

/// What a function of the priming is taken to retain, in kibibytes, before anything of the run is
/// known: a chunk that grows by more than the headroom cannot be stopped, since the ladder acts
/// between chunks, so the first one is small.
pub const FIRST_GROWTH_KIB: u64 = 4 * 1024;

/// How many functions the next chunk may take: as many as fit in half of `headroom_kib` (the
/// estimate is rough) at `growth_kib_per_function`, the growth the last chunk showed, and at least
/// one; never more than `usual`.
#[instrument(level = "trace")]
pub fn functions_within(headroom_kib: i64, growth_kib_per_function: u64, usual: usize) -> usize {
    let fitting = u64::try_from(headroom_kib).unwrap_or(0) / growth_kib_per_function.max(1) / 2;
    usize::try_from(fitting)
        .unwrap_or(usize::MAX)
        .clamp(1, usual.max(1))
}

/// The rung the watchdog last put the run on.
static LEVEL: AtomicU8 = AtomicU8::new(0);

/// The rung a test asks for.
static LEVEL_FORCED: AtomicU8 = AtomicU8::new(0);

/// Pressure a test asks for, so that what the work does under it can be seen without a run that
/// uses that much memory.
static PRESSURE_FORCED: AtomicBool = AtomicBool::new(false);

/// The share of the limit, in percent, below which a run that was under pressure is relieved of it.
/// Lower than [`EASE_OFF_PERCENT`], so the work does not alternate on every sample around one figure.
pub const RELIEF_PERCENT: u64 = 45;

/// The share of the limit, in percent, from which the work gives back what it can compute again
/// and takes smaller steps, so the watchdog's stop is the last resort and not the usual end.
pub const EASE_OFF_PERCENT: u64 = 60;

/// How often the watchdog reads the process's memory.
const SAMPLE_EVERY: Duration = Duration::from_millis(100);

/// A memory limit as it was written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemoryLimit {
    /// A number of bytes; zero is no limit.
    Bytes(u64),
    /// A share of the memory available when the run starts, from 0 (no limit) to 100.
    Percent(f64),
}

/// A limit worked out: how many bytes, and how it came to be, for the message that says it was
/// passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    bytes: u64,
    described: String,
    /// What the pressure ladder works to, when that is less than the limit (zero: the limit).
    target: u64,
}

impl Budget {
    /// The limit in bytes; zero is none.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// What the pressure ladder works to, in bytes: the target if one was set and is below the
    /// limit, else the limit. The run is stopped at the limit and not at the target, so a run can
    /// be pressed hard at a small target and its behaviour seen all the way to the limit.
    #[must_use]
    pub fn ladder_bytes(&self) -> u64 {
        if self.target == 0 || (self.bytes != 0 && self.target >= self.bytes) {
            self.bytes
        } else {
            self.target
        }
    }

    /// This budget less `bytes` that something else of the run holds: the driver of a run in
    /// processes is part of the run, and what it holds is not the child's to spend. Never less than
    /// a mebibyte, so that a driver that holds more than the budget does not make it none.
    #[must_use]
    pub fn reserving(mut self, bytes: u64) -> Self {
        const LEAST: u64 = 1 << 20;
        if self.bytes != 0 {
            self.bytes = self.bytes.saturating_sub(bytes).max(LEAST);
        }
        if self.target != 0 {
            self.target = self.target.saturating_sub(bytes).max(LEAST);
        }
        self
    }

    /// This budget with the ladder working to `target` (see [`Budget::ladder_bytes`]); a target
    /// that is no limit leaves it as it was.
    #[must_use]
    pub fn with_target(mut self, target: &Budget) -> Self {
        self.target = target.bytes;
        self
    }
}

/// Parse a limit: a size such as `8G`, `512M`, `64k` or a plain number of bytes (binary units,
/// either case), or a share such as `75%` of the memory available. `0` and `0%` are no limit.
#[instrument(level = "trace", err(level = "debug"))]
pub fn parse_limit(text: &str) -> Result<MemoryLimit, String> {
    let text = text.trim();
    if let Some(percent) = text.strip_suffix('%') {
        let percent: f64 = percent
            .trim()
            .parse()
            .map_err(|_| format!("`{text}` is not a share such as 75%"))?;
        return if (0.0..=100.0).contains(&percent) {
            Ok(MemoryLimit::Percent(percent))
        } else {
            Err(format!("`{text}` is not between 0% and 100%"))
        };
    }
    parse_size(text).map(MemoryLimit::Bytes)
}

/// Parse a size such as `8G`, `512M`, `64k` or a plain number of bytes into bytes.
#[instrument(level = "trace", err(level = "debug"))]
pub fn parse_size(text: &str) -> Result<u64, String> {
    let text = text.trim();
    let (number, shift) = match text.chars().last() {
        Some('k' | 'K') => (&text[..text.len() - 1], 10),
        Some('m' | 'M') => (&text[..text.len() - 1], 20),
        Some('g' | 'G') => (&text[..text.len() - 1], 30),
        Some('t' | 'T') => (&text[..text.len() - 1], 40),
        _ => (text, 0),
    };
    let number: u64 = number
        .trim()
        .parse()
        .map_err(|_| format!("`{text}` is not a size such as 8G, 512M, 75% or 0"))?;
    number
        .checked_mul(1 << shift)
        .ok_or_else(|| format!("`{text}` is too large"))
}

/// The memory available for a new program, in bytes, from the text of `/proc/meminfo`.
#[instrument(level = "trace", skip(meminfo))]
pub fn parse_meminfo(meminfo: &str) -> Option<u64> {
    let line = meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemAvailable:"))?;
    let kibibytes: u64 = line.split_whitespace().next()?.parse().ok()?;
    kibibytes.checked_mul(1024)
}

/// The memory available now, where the platform says.
#[instrument(level = "debug")]
#[hotpath::measure]
pub fn available_memory() -> Option<u64> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

/// The budget `limit` is, given the memory `available` (if it is known).
#[instrument(level = "debug")]
#[hotpath::measure]
pub fn budget_of(limit: MemoryLimit, available: Option<u64>) -> Budget {
    match limit {
        MemoryLimit::Bytes(0) | MemoryLimit::Percent(0.0) => Budget {
            bytes: 0,
            described: "no limit".to_owned(),
            target: 0,
        },
        MemoryLimit::Bytes(bytes) => Budget {
            bytes,
            described: gibibytes(bytes),
            target: 0,
        },
        MemoryLimit::Percent(percent) => match available {
            #[expect(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a share of a figure that is only an estimate"
            )]
            Some(available) => {
                let bytes = (available as f64 * percent / 100.0) as u64;
                Budget {
                    bytes,
                    described: format!(
                        "{} ({percent}% of the {} available when it started)",
                        gibibytes(bytes),
                        gibibytes(available)
                    ),
                    target: 0,
                }
            }
            None => Budget {
                bytes: FALLBACK_BYTES,
                described: format!(
                    "{} (the memory available could not be read, so {percent}% could not be worked out)",
                    gibibytes(FALLBACK_BYTES)
                ),
                target: 0,
            },
        },
    }
}

#[expect(clippy::cast_precision_loss, reason = "a figure for a person to read")]
fn gibibytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / f64::from(1u32 << 30))
}

/// The budget for a run: what the command line (or `CORDIAL_MEMORY_LIMIT`) gave, else what
/// `cordial.toml` says, else the default. A value in `cordial.toml` that is not a limit is said
/// to be one and the default is used: a bad setting must not cost the whole configuration.
#[instrument(level = "debug")]
#[hotpath::measure]
pub fn resolve(flag: Option<MemoryLimit>, configured: Option<&str>) -> Budget {
    resolve_with(flag, configured, available_memory())
}

/// [`resolve`], given the memory available.
#[instrument(level = "debug")]
#[hotpath::measure]
pub fn resolve_with(
    flag: Option<MemoryLimit>,
    configured: Option<&str>,
    available: Option<u64>,
) -> Budget {
    if let Some(flag) = flag {
        return budget_of(flag, available);
    }
    if let Some(text) = configured {
        match parse_limit(text) {
            Ok(limit) => return budget_of(limit, available),
            Err(reason) => {
                let _ = writeln!(
                    crate::host::stderr(),
                    "cordial.toml: memory_limit: {reason}; using {DEFAULT_MEMORY_LIMIT}"
                );
            }
        }
    }
    let default = parse_limit(DEFAULT_MEMORY_LIMIT).unwrap_or(MemoryLimit::Bytes(FALLBACK_BYTES));
    budget_of(default, available)
}

/// Whether a process using `resident_kib` is over `limit` bytes. A limit of zero is none.
#[instrument(level = "trace")]
pub fn exceeds(resident_kib: u64, limit: u64) -> bool {
    limit != 0 && resident_kib.saturating_mul(1024) > limit
}

/// Whether a process using `resident_kib` has reached [`EASE_OFF_PERCENT`] of `limit` bytes. A
/// limit of zero is none.
#[instrument(level = "trace")]
pub fn nearing(resident_kib: u64, limit: u64) -> bool {
    limit != 0
        && resident_kib.saturating_mul(1024).saturating_mul(100)
            >= limit.saturating_mul(EASE_OFF_PERCENT)
}

/// Whether a process using `resident_kib` is back under [`RELIEF_PERCENT`] of `limit` bytes.
#[instrument(level = "trace")]
pub fn relieved(resident_kib: u64, limit: u64) -> bool {
    limit == 0
        || resident_kib.saturating_mul(1024).saturating_mul(100)
            < limit.saturating_mul(RELIEF_PERCENT)
}

/// Whether the run is nearing its limit now: the work gives back what it can compute again and
/// takes smaller steps while this is so.
#[instrument(level = "trace")]
pub fn pressured() -> bool {
    PRESSURE.load(Ordering::Relaxed) || PRESSURE_FORCED.load(Ordering::Relaxed)
}

/// Whether the run is nearing its limit as it is now: read from the process, and not the watchdog's
/// last judgement, which is up to a sample old (what an eviction has just given back is not in it).
#[instrument(level = "trace")]
pub fn nearing_now() -> bool {
    PRESSURE_FORCED.load(Ordering::Relaxed)
        || meter_kib().is_some_and(|held| nearing(held, LIMIT_BYTES.load(Ordering::Relaxed)))
}

/// Whether the environment asked for a cap on what the LRU of each query keeps.
static LRU_CAP_ASKED: AtomicBool = AtomicBool::new(false);

/// Let the LRU of each query hold what it has seen, as long as the run is not short of memory and nothing asked for a
/// cap: the marks on the queries (512 expansions, 1,024 id maps, 512 bodies) are what an editor keeps, and a write
/// (every flip of a cfg is one) drops everything over them, which each query that then needs a value makes again.
/// `cap_asked` says that the environment asked for a cap.
#[instrument(level = "debug")]
pub fn hold_lru_values_unless_pressed(cap_asked: bool) {
    LRU_CAP_ASKED.store(cap_asked, Ordering::Relaxed);
    apply_lru_policy();
}

/// Say to the queries whether they may drop what they hold: not while nothing asked for a cap and the run has the
/// memory. Said again whenever the pressure changes.
#[instrument(level = "debug")]
pub(crate) fn apply_lru_policy() {
    salsa::lift_lru_capacities(!LRU_CAP_ASKED.load(Ordering::Relaxed) && !pressured());
}

/// Make the run feel pressure (or stop feeling it), for the tests of what the work does under it.
#[instrument(level = "debug")]
pub fn feel_pressure(on: bool) {
    PRESSURE_FORCED.store(on, Ordering::Relaxed);
    LEVEL_FORCED.store(u8::from(on), Ordering::Relaxed);
    apply_lru_policy();
}

/// How much more than at the last release a process holds before it lets go of the scaffolding
/// again while it builds the definition maps (in kibibytes): a release walks every memo, so it is
/// made when enough has built up to be worth it.
static SCAFFOLDING_STEP_KIB: AtomicU64 = AtomicU64::new(96 * 1024);

/// Change how much is built up between releases of scaffolding: what a test of the release asks
/// for, since a workspace small enough to test does not build up the default.
pub fn release_scaffolding_every_kib(step: u64) {
    SCAFFOLDING_STEP_KIB.store(step, Ordering::Relaxed);
}

/// How much is built up between releases of scaffolding.
#[must_use]
pub fn scaffolding_step_kib() -> u64 {
    SCAFFOLDING_STEP_KIB.load(Ordering::Relaxed)
}

/// Make the run feel the pressure of `level`, for the tests of what the work does on each rung.
#[instrument(level = "debug")]
pub fn feel_pressure_at(level: Pressure) {
    PRESSURE_FORCED.store(level > Pressure::None, Ordering::Relaxed);
    LEVEL_FORCED.store(level as u8, Ordering::Relaxed);
    apply_lru_policy();
}

/// The rung the run is on now: the watchdog's, or the one a test asked for if it is higher.
#[instrument(level = "trace")]
pub fn pressure() -> Pressure {
    Pressure::from_bits(
        LEVEL
            .load(Ordering::Relaxed)
            .max(LEVEL_FORCED.load(Ordering::Relaxed)),
    )
}

/// The next judgement of pressure given the last one: it begins at [`EASE_OFF_PERCENT`] of the
/// limit and ends below [`RELIEF_PERCENT`].
#[instrument(level = "trace")]
pub fn judge(was_pressured: bool, resident_kib: u64, limit: u64) -> bool {
    if was_pressured {
        !relieved(resident_kib, limit)
    } else {
        nearing(resident_kib, limit)
    }
}

/// The budget for the pressure ladder's target: what `--memory-target` (or `CORDIAL_MEMORY_TARGET`)
/// gave, else what `cordial.toml` says, else none, so that the ladder works to the limit.
#[instrument(level = "debug")]
#[hotpath::measure]
pub fn resolve_target(
    limit: Budget,
    flag: Option<MemoryLimit>,
    configured: Option<&str>,
    available: Option<u64>,
) -> Budget {
    let target = flag.or_else(|| {
        configured.and_then(|text| match parse_limit(text) {
            Ok(target) => Some(target),
            Err(reason) => {
                let _ = writeln!(
                    crate::host::stderr(),
                    "cordial.toml: memory_target: {reason}; using the limit"
                );
                None
            }
        })
    });
    match target {
        Some(target) => limit.with_target(&budget_of(target, available)),
        None => limit,
    }
}

/// Start the watchdog for `budget`. It lives as long as the process.
#[instrument(level = "debug")]
#[hotpath::measure]
pub fn enforce(budget: Budget) {
    LIMIT_BYTES.store(budget.ladder_bytes(), Ordering::Relaxed);
    if budget.bytes == 0 {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("cordial-memory-limit".to_owned())
        .spawn(move || watch(&budget));
    if let Err(error) = spawned {
        // The run goes on without the bound; it is said so that nobody relies on it.
        tracing::warn!(error = ?error, "could not start the memory limit watchdog");
    }
}

#[instrument(level = "debug")]
#[hotpath::measure]
fn watch(budget: &Budget) {
    loop {
        std::thread::sleep(SAMPLE_EVERY);
        let Some(memory) = crate::process_memory::sample() else {
            // No reading on this platform: nothing to bound by.
            return;
        };
        let held = meter_of(memory.resident_kib());
        PEAK_HELD_KIB.fetch_max(held, Ordering::Relaxed);
        if memory.resident_kib()
            > PEAK_RESIDENT_KIB.fetch_max(memory.resident_kib(), Ordering::Relaxed)
        {
            HELD_AT_PEAK_RESIDENT_KIB.store(held, Ordering::Relaxed);
        }
        let rung = judge_level(
            Pressure::from_bits(LEVEL.load(Ordering::Relaxed)),
            held,
            budget.ladder_bytes(),
        );
        if rung as u8 != LEVEL.load(Ordering::Relaxed) {
            LEVEL.store(rung as u8, Ordering::Relaxed);
            // The workers of a round that is running are narrowed (or widened) now, at the next item
            // each starts, and not when the next round begins.
            crate::allow_workers(workers_at(crate::pool_size(), rung));
            tracing::info!(
                target: "cordial::eviction",
                rung = rung as u8,
                held_kib = held,
                resident_kib = memory.resident_kib(),
                phase = %crate::LiveStatus::global().phase(),
                status = %crate::LiveStatus::global().line(),
                "the run's rung of the pressure ladder changed"
            );
        }
        let was = PRESSURE.load(Ordering::Relaxed);
        let now = judge(was, held, budget.ladder_bytes());
        if now != was {
            PRESSURE.store(now, Ordering::Relaxed);
            apply_lru_policy();
            tracing::info!(
                target: "cordial::eviction",
                pressured = now,
                held_kib = held,
                resident_kib = memory.resident_kib(),
                "the run's memory pressure changed"
            );
        }
        if resumable() && checkpoint_due(memory.resident_kib(), budget.ladder_bytes()) {
            request_checkpoint();
        }
        if over_budget(held, memory.resident_kib(), budget.bytes) {
            stop(held, memory.resident_kib(), budget);
        }
    }
}

/// Say that the run is over its limit, and where it was, and exit.
fn stop(held_kib: u64, resident_kib: u64, budget: &Budget) -> ! {
    let status = crate::LiveStatus::global();
    let _ = writeln!(
        crate::host::stderr(),
        "\ncordial stopped itself: it was holding {} ({} resident), over its limit of {}, during \"{}\" [{}].\n\
         Raise the limit with --memory-limit (for example 90% or 16G; 0 for none), or set \
         memory_limit in cordial.toml or CORDIAL_MEMORY_LIMIT.",
        gibibytes(held_kib.saturating_mul(1024)),
        gibibytes(resident_kib.saturating_mul(1024)),
        budget.described,
        status.phase(),
        status.line()
    );
    crate::run_metrics::write_ended("memory limit", &status.phase());
    log_footprint("stopped by the limit");
    // The driver of a run in processes is told where this one was stopped, so that it can say what
    // the budget was too small for.
    crate::report_progress(crate::Progress::Stopped {
        resident_kib,
        held_kib,
    });
    // The profile of the run that was stopped is the one that says where its memory went.
    crate::profiling::finish(crate::profiling::FinishReason::Watchdog);
    std::process::exit(MEMORY_LIMIT_EXIT)
}
