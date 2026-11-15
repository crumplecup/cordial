//! A bound on how much memory a run may use, enforced by the process itself.
//!
//! Analysing a large workspace takes many gigabytes, and a run that grows without bound can
//! exhaust the machine before anything notices. A watchdog thread reads the process's resident
//! memory (see [`crate::process_memory`]) and, when it passes the limit, says where the run was
//! and exits, so that the limit is a decision the person made and not whatever the system does
//! when it runs out.

use std::io::Write as _;
use std::time::Duration;

use tracing::instrument;

/// The limit when none is given: eight gibibytes.
pub const DEFAULT_MEMORY_LIMIT: &str = "8G";

/// The exit status of a run that stopped itself for using too much memory.
pub const MEMORY_LIMIT_EXIT: i32 = 75;

/// How often the watchdog reads the process's memory.
const SAMPLE_EVERY: Duration = Duration::from_millis(100);

/// Parse a size such as `8G`, `512M`, `64k` or a plain number of bytes into bytes. `0` is no limit.
/// Units are binary and either case.
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
        .map_err(|_| format!("`{text}` is not a size such as 8G, 512M or 0"))?;
    number
        .checked_mul(1 << shift)
        .ok_or_else(|| format!("`{text}` is too large"))
}

/// The limit in bytes for a run: what the command line (or `CORDIAL_MEMORY_LIMIT`) gave, else
/// what `cordial.toml` says, else the default. A value in `cordial.toml` that is not a size is
/// said to be one and the default is used: a bad setting must not cost the whole configuration.
#[instrument(level = "debug")]
pub fn resolve(flag: Option<u64>, configured: Option<&str>) -> u64 {
    if let Some(flag) = flag {
        return flag;
    }
    if let Some(text) = configured {
        match parse_size(text) {
            Ok(limit) => return limit,
            Err(reason) => {
                let _ = writeln!(
                    crate::host::stderr(),
                    "cordial.toml: memory_limit: {reason}; using {DEFAULT_MEMORY_LIMIT}"
                );
            }
        }
    }
    parse_size(DEFAULT_MEMORY_LIMIT).unwrap_or(8 << 30)
}

/// Whether a process using `resident_kib` is over `limit` bytes. A limit of zero is none.
#[instrument(level = "trace")]
pub fn exceeds(resident_kib: u64, limit: u64) -> bool {
    limit != 0 && resident_kib.saturating_mul(1024) > limit
}

/// Start the watchdog for `limit` bytes (zero for none). It lives as long as the process.
#[instrument(level = "debug")]
#[hotpath::measure]
pub fn enforce(limit: u64) {
    if limit == 0 {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("cordial-memory-limit".to_owned())
        .spawn(move || watch(limit));
    if let Err(error) = spawned {
        // The run goes on without the bound; it is said so that nobody relies on it.
        tracing::warn!(error = ?error, "could not start the memory limit watchdog");
    }
}

#[instrument(level = "debug")]
fn watch(limit: u64) {
    loop {
        std::thread::sleep(SAMPLE_EVERY);
        let Some(memory) = crate::process_memory::sample() else {
            // No reading on this platform: nothing to bound by.
            return;
        };
        if exceeds(memory.resident_kib(), limit) {
            stop(memory.resident_kib(), limit);
        }
    }
}

/// Say that the run is over its limit, and where it was, and exit.
fn stop(resident_kib: u64, limit: u64) -> ! {
    let status = crate::LiveStatus::global();
    #[expect(clippy::cast_precision_loss, reason = "a figure for a person to read")]
    let (used, allowed) = (
        resident_kib as f64 / (1024.0 * 1024.0),
        limit as f64 / f64::from(1u32 << 30),
    );
    let _ = writeln!(
        crate::host::stderr(),
        "\ncordial stopped itself: it was using {used:.1} GB of memory, over its limit of \
         {allowed:.1} GB, during \"{}\" [{}].\nRaise the limit with --memory-limit (for \
         example --memory-limit 16G; 0 for none), or set CORDIAL_MEMORY_LIMIT.",
        status.phase(),
        status.line()
    );
    std::process::exit(MEMORY_LIMIT_EXIT)
}
