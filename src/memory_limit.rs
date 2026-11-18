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
use std::sync::atomic::{AtomicU64, Ordering};
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

/// Whether this process, as it is now, is nearing the limit of this run.
#[instrument(level = "trace")]
pub fn nearing_this_runs_limit() -> bool {
    crate::process_memory::sample()
        .is_some_and(|memory| nearing(memory.resident_kib(), LIMIT_BYTES.load(Ordering::Relaxed)))
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
    std::process::exit(MEMORY_LIMIT_EXIT)
}
