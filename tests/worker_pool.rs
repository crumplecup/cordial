//! The pool of a pass is one Tokio runtime: every item of a round is a task of a `JoinSet` that
//! waits at a gate for its turn, on a blocking pool that has no more threads than the cores. The
//! gate can be narrowed while a round runs, a round that is told to stop leaves the last of its
//! items, and a panic in an item is the caller's.
//!
//! The pool and its gate are the process's, so every test is its own process (nextest).

use cordial::{allow_workers, allowed_workers, pool_size, run_items};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn items(count: usize) -> Arc<[usize]> {
    (0..count).collect::<Vec<_>>().into()
}

fn resources(count: usize) -> Vec<usize> {
    (0..count).collect()
}

fn never(_: usize) -> bool {
    false
}

#[test]
fn many_rounds_run_on_no_more_threads_than_the_pool_has() {
    let seen: Arc<Mutex<BTreeSet<String>>> = Arc::new(Mutex::new(BTreeSet::new()));
    let ran = Arc::new(AtomicUsize::new(0));
    let all = items(pool_size() * 4);
    for _ in 0..200 {
        let (seen, ran) = (Arc::clone(&seen), Arc::clone(&ran));
        let done = run_items(
            &all,
            0..all.len(),
            resources(pool_size()),
            move |_, _| {
                let thread = std::thread::current();
                let id = format!("{:?} {}", thread.id(), thread.name().unwrap_or("?"));
                seen.lock().map(|mut seen| seen.insert(id)).ok();
                ran.fetch_add(1, Ordering::Relaxed);
            },
            never,
        );
        assert_eq!(done, all.len());
    }
    assert_eq!(
        ran.load(Ordering::Relaxed),
        200 * all.len(),
        "every item ran"
    );
    let seen = seen.lock().map(|seen| seen.clone()).unwrap_or_default();
    assert!(
        seen.len() <= pool_size() + 1 && seen.iter().all(|name| name.ends_with("cordial-prime")),
        "200 rounds on at most {} threads of the runtime (its worker and a thread for each core): {seen:?}",
        pool_size() + 1
    );
}

/// Runs a round of items that each sleep, and says the most that ran at the same moment.
fn widest(all: &Arc<[usize]>, workers: usize) -> usize {
    allow_workers(workers);
    let (now, most) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let (now_in, most_in) = (Arc::clone(&now), Arc::clone(&most));
    run_items(
        all,
        0..all.len(),
        resources(pool_size()),
        move |_, _| {
            let running = now_in.fetch_add(1, Ordering::SeqCst) + 1;
            most_in.fetch_max(running, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(15));
            now_in.fetch_sub(1, Ordering::SeqCst);
        },
        never,
    );
    most.load(Ordering::SeqCst)
}

#[test]
fn the_gate_says_how_many_items_run_at_once() {
    let all = items(24);
    assert_eq!(widest(&all, 1), 1, "one at a time");
    let two = widest(&all, 2);
    assert!(two <= 2.min(pool_size()), "no more than two: {two}");
    assert_eq!(allowed_workers(), 2.min(pool_size()));
    if pool_size() >= 4 {
        let four = widest(&all, 4);
        assert!(
            four > 2 && four <= 4,
            "and widening it lets more run: {four}"
        );
    }
}

#[test]
fn the_gate_narrows_while_a_round_is_running() {
    if pool_size() < 4 {
        return;
    }
    allow_workers(4);
    let all = items(80);
    let (now, started) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let late_widths: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
    let (now_in, started_in, late_in) = (
        Arc::clone(&now),
        Arc::clone(&started),
        Arc::clone(&late_widths),
    );
    run_items(
        &all,
        0..all.len(),
        resources(pool_size()),
        move |_, item| {
            let running = now_in.fetch_add(1, Ordering::SeqCst) + 1;
            let order = started_in.fetch_add(1, Ordering::SeqCst);
            if order == 8 {
                // The ladder reaches its last rung in the middle of the round.
                allow_workers(1);
            }
            if *item >= 60 {
                late_in.lock().map(|mut late| late.push(running)).ok();
            }
            std::thread::sleep(Duration::from_millis(10));
            now_in.fetch_sub(1, Ordering::SeqCst);
        },
        never,
    );
    let late = late_widths
        .lock()
        .map(|late| late.clone())
        .unwrap_or_default();
    assert_eq!(late.len(), 20);
    assert!(
        late.iter().all(|width| *width == 1),
        "after the gate was narrowed, the items that came later ran one at a time: {late:?}"
    );
}

#[test]
fn a_round_that_is_told_to_stop_leaves_the_last_of_its_items() {
    allow_workers(pool_size());
    let all = items(50);
    let ran: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));
    let ran_in = Arc::clone(&ran);
    let done = run_items(
        &all,
        10..50,
        resources(pool_size()),
        move |_, item| {
            ran_in.lock().map(|mut ran| ran.push(*item)).ok();
        },
        |taken| taken >= 8,
    );
    assert_eq!(done, 8, "eight were taken, and the round says so");
    let mut ran = ran.lock().map(|ran| ran.clone()).unwrap_or_default();
    ran.sort_unstable();
    assert_eq!(
        ran,
        (10..18).collect::<Vec<_>>(),
        "the first eight of the range"
    );
}

#[test]
fn each_running_item_is_lent_a_resource_that_no_other_has() {
    allow_workers(pool_size());
    let all = items(40);
    let held: Arc<Mutex<BTreeSet<usize>>> = Arc::new(Mutex::new(BTreeSet::new()));
    let clashes = Arc::new(AtomicUsize::new(0));
    let (held_in, clashes_in) = (Arc::clone(&held), Arc::clone(&clashes));
    run_items(
        &all,
        0..all.len(),
        resources(pool_size()),
        move |resource, _| {
            if !held_in
                .lock()
                .map(|mut held| held.insert(*resource))
                .unwrap_or(false)
            {
                clashes_in.fetch_add(1, Ordering::SeqCst);
            }
            std::thread::sleep(Duration::from_millis(5));
            held_in.lock().map(|mut held| held.remove(resource)).ok();
        },
        never,
    );
    assert_eq!(
        clashes.load(Ordering::SeqCst),
        0,
        "a resource is never in two hands"
    );
}

#[test]
fn an_item_that_panics_panics_the_round_and_what_was_lent_is_dropped() {
    allow_workers(pool_size());
    let lent = Arc::new(());
    let all = items(12);
    let given: Vec<Arc<()>> = (0..pool_size()).map(|_| Arc::clone(&lent)).collect();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_items(
            &all,
            0..all.len(),
            given,
            |_, item| {
                assert_ne!(*item, 3, "the bad item");
            },
            never,
        )
    }));
    let payload = outcome.expect_err("the round panics");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_owned())
        })
        .unwrap_or_default();
    assert!(message.contains("the bad item"), "{message}");
    assert_eq!(
        Arc::strong_count(&lent),
        1,
        "the round returned holding nothing, so the caller can take what it lent"
    );
}
