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
    let resident = i64::try_from(crate::process_memory::sample()?.resident_kib()).ok()?;
    let line = i64::try_from(limit / 1024 * BEGINS_AT[1] / 100).ok()?;
    Some(line - resident)
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
}

impl Budget {
    /// The limit in bytes; zero is none.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
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
fn available_memory() -> Option<u64> {
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
        },
        MemoryLimit::Bytes(bytes) => Budget {
            bytes,
            described: gibibytes(bytes),
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
                }
            }
            None => Budget {
                bytes: FALLBACK_BYTES,
                described: format!(
                    "{} (the memory available could not be read, so {percent}% could not be worked out)",
                    gibibytes(FALLBACK_BYTES)
                ),
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
        || crate::process_memory::sample().is_some_and(|memory| {
            nearing(memory.resident_kib(), LIMIT_BYTES.load(Ordering::Relaxed))
        })
}

/// Make the run feel pressure (or stop feeling it), for the tests of what the work does under it.
#[instrument(level = "debug")]
pub fn feel_pressure(on: bool) {
    PRESSURE_FORCED.store(on, Ordering::Relaxed);
    LEVEL_FORCED.store(u8::from(on), Ordering::Relaxed);
}

/// Make the run feel the pressure of `level`, for the tests of what the work does on each rung.
#[instrument(level = "debug")]
pub fn feel_pressure_at(level: Pressure) {
    PRESSURE_FORCED.store(level > Pressure::None, Ordering::Relaxed);
    LEVEL_FORCED.store(level as u8, Ordering::Relaxed);
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

/// Start the watchdog for `budget`. It lives as long as the process.
#[instrument(level = "debug")]
#[hotpath::measure]
pub fn enforce(budget: Budget) {
    LIMIT_BYTES.store(budget.bytes, Ordering::Relaxed);
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
        let rung = judge_level(
            Pressure::from_bits(LEVEL.load(Ordering::Relaxed)),
            memory.resident_kib(),
            budget.bytes,
        );
        if rung as u8 != LEVEL.load(Ordering::Relaxed) {
            LEVEL.store(rung as u8, Ordering::Relaxed);
            tracing::info!(
                target: "cordial::eviction",
                rung = rung as u8,
                resident_kib = memory.resident_kib(),
                "the run's rung of the pressure ladder changed"
            );
        }
        let was = PRESSURE.load(Ordering::Relaxed);
        let now = judge(was, memory.resident_kib(), budget.bytes);
        if now != was {
            PRESSURE.store(now, Ordering::Relaxed);
            tracing::info!(
                target: "cordial::eviction",
                pressured = now,
                resident_kib = memory.resident_kib(),
                "the run's memory pressure changed"
            );
        }
        if exceeds(memory.resident_kib(), budget.bytes) {
            stop(memory.resident_kib(), budget);
        }
    }
}

/// Say that the run is over its limit, and where it was, and exit.
fn stop(resident_kib: u64, budget: &Budget) -> ! {
    let status = crate::LiveStatus::global();
    let _ = writeln!(
        crate::host::stderr(),
        "\ncordial stopped itself: it was using {}, over its limit of {}, during \"{}\" [{}].\n\
         Raise the limit with --memory-limit (for example 90% or 16G; 0 for none), or set \
         memory_limit in cordial.toml or CORDIAL_MEMORY_LIMIT.",
        gibibytes(resident_kib.saturating_mul(1024)),
        budget.described,
        status.phase(),
        status.line()
    );
    crate::run_metrics::write_ended("memory limit", &status.phase());
    // The profile of the run that was stopped is the one that says where its memory went.
    crate::profiling::finish();
    std::process::exit(MEMORY_LIMIT_EXIT)
}
