//! Memory a program frees is given back to the system, so that the resident memory the watchdog reads
//! follows what the program holds.
//!
//! The pattern is the analysis's own: many small objects made on worker threads (trees, expansions,
//! interned values) and dropped, later, on another thread. An allocator that keeps what it was given
//! in per-thread arenas can hold the pages after the objects are gone, and the watchdog would stop a
//! run for memory it no longer uses.

use std::time::Duration;

/// How many objects each of the workers makes, and how big each is.
const WORKERS: usize = 8;
const PER_WORKER: usize = 400_000;
const OBJECT_BYTES: usize = 96;

fn resident_kib() -> u64 {
    cordial::memory_sample().map_or(0, |sample| sample.resident_kib())
}

#[test]
fn memory_made_on_workers_and_dropped_on_another_thread_is_given_back() {
    let before = resident_kib();
    let held: Vec<Vec<Box<[u8; OBJECT_BYTES]>>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..WORKERS)
            .map(|worker| {
                scope.spawn(move || {
                    (0..PER_WORKER)
                        .map(|index| {
                            Box::new(
                                [u8::try_from((worker + index) % 250).unwrap_or(1) + 1;
                                    OBJECT_BYTES],
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or_default())
            .collect()
    });
    let grown = resident_kib().saturating_sub(before);
    assert!(
        grown > 200_000,
        "the objects are resident while held ({grown} KiB grown): the test measures nothing otherwise"
    );

    drop(held);
    // Allocators give memory back a little after it is freed.
    std::thread::sleep(Duration::from_millis(500));
    let kept = resident_kib().saturating_sub(before);
    assert!(
        kept < grown / 4,
        "most of what was freed is given back: {kept} KiB of the {grown} KiB are still resident"
    );
}
