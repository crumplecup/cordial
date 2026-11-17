//! Where the program held the most memory: which queries held it, and what the run was doing.
//!
//! A reading at each checkpoint says how much is held there, not where the most was held, which
//! is between them. A thread reads the allocator's counters a few times a millisecond and keeps
//! the whole reading, by query, whenever the total is the highest it has seen.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::instrument;

/// How often the counters are read.
const EVERY: Duration = Duration::from_millis(5);

/// The program as it was when it held the most.
struct Sample {
    total: i64,
    stage: String,
    phase: String,
    executed: u64,
    elapsed_ms: u64,
    by_tag: Vec<(u32, i64)>,
}

/// A query's memory falling by this much between two readings is kept as a release.
const RELEASE_FROM: i64 = 16 * 1024 * 1024;
/// How many releases are kept.
const RELEASES: usize = 200;

/// Memory a query held less of than a reading before.
struct Release {
    tag: u32,
    bytes: i64,
    stage: String,
    phase: String,
    elapsed_ms: u64,
}

static RELEASED: Mutex<Vec<Release>> = Mutex::new(Vec::new());
static HIGHEST: Mutex<Option<Sample>> = Mutex::new(None);
static STAGE: Mutex<String> = Mutex::new(String::new());

/// Say what the run is about to do, so that a reading names it: the last thing said before the
/// reading is what was running.
pub(crate) fn note_stage(stage: impl Into<String>) {
    if let Ok(mut current) = STAGE.lock() {
        *current = stage.into();
    }
}

/// Start reading the counters, if the program has them. Once per process.
#[instrument(level = "debug")]
pub fn start_sampling() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    if salsa::held_in_all().is_none() {
        return;
    }
    STARTED.call_once(|| {
        let began = Instant::now();
        let spawned = std::thread::Builder::new()
            .name("cordial-heap-sampler".to_owned())
            .spawn(move || {
                let mut highest = 0_i64;
                let mut before: std::collections::HashMap<u32, i64> = Default::default();
                loop {
                    std::thread::sleep(EVERY);
                    let Some(total) = salsa::held_in_all() else {
                        return;
                    };
                    let now: std::collections::HashMap<u32, i64> =
                        salsa::held_by_tag().into_iter().collect();
                    for (tag, held) in &before {
                        let less = held - now.get(tag).copied().unwrap_or(0);
                        if less >= RELEASE_FROM
                            && let Ok(mut all) = RELEASED.lock()
                            && all.len() < RELEASES
                        {
                            all.push(Release {
                                tag: *tag,
                                bytes: less,
                                stage: STAGE.lock().map(|stage| stage.clone()).unwrap_or_default(),
                                phase: crate::LiveStatus::global().phase(),
                                elapsed_ms: u64::try_from(began.elapsed().as_millis())
                                    .unwrap_or(u64::MAX),
                            });
                        }
                    }
                    before = now;
                    if total <= highest {
                        continue;
                    }
                    highest = total;
                    let sample = Sample {
                        total,
                        stage: STAGE.lock().map(|stage| stage.clone()).unwrap_or_default(),
                        phase: crate::LiveStatus::global().phase(),
                        executed: crate::LiveStatus::global().executed(),
                        elapsed_ms: u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX),
                        by_tag: salsa::held_by_tag(),
                    };
                    if let Ok(mut kept) = HIGHEST.lock() {
                        *kept = Some(sample);
                    }
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(error = ?error, "could not start the heap sampler");
        }
    });
}

/// Say, as events, what was freed and by whom: each release of a query's memory with when and in
/// what stage, and the pairs of the query that allocated the most freed and the one executing
/// when it was freed.
#[instrument(level = "debug", skip(db))]
pub(crate) fn report_releases(db: &dyn salsa::Database) {
    if let Ok(all) = RELEASED.lock() {
        for (index, release) in all.iter().enumerate() {
            // A name for each, in the order they came: events of one name are added up, and a
            // number would be dropped from it.
            let order: String = [index / 26, index % 26]
                .iter()
                .filter_map(|letter| char::from_u32(u32::from(b'a') + u32::try_from(*letter).ok()?))
                .collect();
            tracing::info!(
                target: "cordial::heap",
                ingredient = format!("released {order} {}", salsa::ingredient_name(db, release.tag)),
                bytes = u64::try_from(release.bytes).unwrap_or(0),
                at_ms = release.elapsed_ms,
                stage = release.stage.as_str(),
                phase = release.phase.as_str(),
                "a query's memory fell"
            );
        }
    }
    let mut pairs = salsa::freed_pairs();
    pairs.sort_by_key(|(_, _, bytes)| std::cmp::Reverse(*bytes));
    for (allocated, freed, bytes) in pairs.into_iter().take(30) {
        tracing::info!(
            target: "cordial::heap",
            ingredient = format!(
                "freed {} while {}",
                salsa::ingredient_name(db, allocated),
                salsa::ingredient_name(db, freed)
            ),
            bytes = u64::try_from(bytes).unwrap_or(0),
            "memory freed, by the query that allocated it and the one executing"
        );
    }
}

/// Say, as events, the highest the program held and by whom, the largest holders first.
#[instrument(level = "debug", skip(db))]
pub(crate) fn report(db: &dyn salsa::Database) {
    let Ok(kept) = HIGHEST.lock() else {
        return;
    };
    let Some(sample) = kept.as_ref() else {
        return;
    };
    tracing::info!(
        target: "cordial::heap",
        ingredient = "peak at",
        bytes = u64::try_from(sample.total).unwrap_or(0),
        elapsed_ms = sample.elapsed_ms,
        queries_executed = sample.executed,
        stage = sample.stage.as_str(),
        phase = sample.phase.as_str(),
        "the most the program held at once"
    );
    let mut rows: Vec<(String, i64)> = sample
        .by_tag
        .iter()
        .map(|(tag, bytes)| (salsa::ingredient_name(db, *tag), *bytes))
        .collect();
    rows.sort_by_key(|(_, bytes)| std::cmp::Reverse(*bytes));
    for (name, bytes) in rows.into_iter().take(25) {
        tracing::info!(
            target: "cordial::heap",
            ingredient = format!("peak held {name}"),
            bytes = u64::try_from(bytes).unwrap_or(0),
            "held by the query when the program held the most"
        );
    }
}
