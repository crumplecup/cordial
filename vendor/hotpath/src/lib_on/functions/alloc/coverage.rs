//! Allocator-safe lifetime coverage counters. No allocation or TLS access.
use std::sync::atomic::{AtomicU64, Ordering};

macro_rules! counters {
    ($($name:ident),* $(,)?) => { $(pub(crate) static $name: AtomicU64 = AtomicU64::new(0);)* };
}
counters! {
    OVERFLOW_THREADS, HIDDEN_ALLOC, HIDDEN_FREE, RECORDER_ALLOC, RECORDER_FREE,
    UNSCOPED_BYTES, UNSCOPED_COUNT, BEFORE_ALLOC, BEFORE_FREE, TLS_ALLOC, TLS_FREE,
    DEPTH_HITS, CROSS_THREAD, INACTIVE, DROPPED,
}

pub(crate) fn add(counter: &AtomicU64, amount: usize) {
    counter.fetch_add(amount as u64, Ordering::Relaxed);
}

pub(crate) fn read(counter: &AtomicU64) -> u64 {
    counter.load(Ordering::Relaxed)
}
