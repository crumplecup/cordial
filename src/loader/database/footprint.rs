//! What the Salsa database holds, by query and by struct ingredient, at the points of a run where it
//! grows.
//!
//! A run's memory is measured from outside: the resident size beside each milestone. That says how much
//! a phase added and not what it added. Asked for with the `CORDIAL_FOOTPRINT` variable, each of the
//! points below also says, for the queries and structs that hold the most, how many memos or slots
//! there are, how many of the memos hold their value in memory, and the bytes Salsa measures (the size
//! of a value as it is stored and of Salsa's own records for it: what a value owns on the heap is not
//! in either). A phase's growth by kind is then the difference between two points.
//!
//! Walking every memo is not free: the walk is timed and reported with the totals.

use ra_ap_ide_db::RootDatabase;
use tracing::instrument;

use crate::SalsaUsage;

/// How many kinds of each class are reported at a point: the rest are in the totals.
const TOP: usize = 30;

/// Say what `db` holds at `phase`, if the run asked.
#[instrument(level = "debug", skip(db))]
pub(crate) fn log(db: &mut RootDatabase, phase: &str) {
    if std::env::var_os(crate::FOOTPRINT_VARIABLE).is_none() {
        return;
    }
    let started = std::time::Instant::now();
    let footprint = crate::storage::salsa_footprint(db);
    let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    let bytes = |usage: &SalsaUsage| (usage.fields_bytes() + usage.metadata_bytes()) as u64;
    for (class, kinds) in [
        ("query", footprint.queries()),
        ("struct", footprint.structs()),
    ] {
        let mut ranked: Vec<&SalsaUsage> = kinds.iter().collect();
        ranked.sort_by_key(|usage| std::cmp::Reverse(bytes(usage)));
        for usage in ranked.iter().take(TOP) {
            tracing::debug!(
                target: "cordial::footprint",
                what = %format!("{phase} {class} {}", usage.name()),
                memos = usage.count() as u64,
                resident = usage.values_resident() as u64,
                slots = usage.slots_claimed() as u64,
                fields_bytes = usage.fields_bytes() as u64,
                metadata_bytes = usage.metadata_bytes() as u64,
                "what a query or struct holds"
            );
        }
        tracing::debug!(
            target: "cordial::footprint",
            what = %format!("{phase} {class} total"),
            kinds = kinds.len() as u64,
            memos = kinds.iter().map(|usage| usage.count() as u64).sum::<u64>(),
            resident = kinds.iter().map(|usage| usage.values_resident() as u64).sum::<u64>(),
            fields_bytes = kinds.iter().map(|usage| usage.fields_bytes() as u64).sum::<u64>(),
            metadata_bytes = kinds.iter().map(|usage| usage.metadata_bytes() as u64).sum::<u64>(),
            micros,
            "what all the queries or structs hold"
        );
    }
}
