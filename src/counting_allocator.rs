//! The system allocator, counting for each thread the bytes it has allocated and not yet freed.
//!
//! A query's execution is on one thread from start to end, so the difference of the count across
//! it is what the query left behind. Memory freed on another thread than the one that allocated
//! it makes a thread's count drift, which is no matter across one query.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static NET: Cell<i64> = const { Cell::new(0) };
}

/// Counts for the program's allocator.
pub struct Counting;

fn add(bytes: usize, grows: bool) {
    let bytes = i64::try_from(bytes).unwrap_or(i64::MAX);
    // Not available while the thread is being torn down: nothing to count then.
    let _ = NET.try_with(|net| net.set(net.get() + if grows { bytes } else { -bytes }));
}

// SAFETY: every method forwards to the system allocator with the arguments it was given.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        add(layout.size(), true);
        // SAFETY: as the caller's contract for `alloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        add(layout.size(), false);
        // SAFETY: as the caller's contract for `dealloc`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        add(layout.size(), true);
        // SAFETY: as the caller's contract for `alloc_zeroed`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        add(layout.size(), false);
        add(new_size, true);
        // SAFETY: as the caller's contract for `realloc`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// The bytes the calling thread has allocated and not freed.
pub fn net_bytes() -> i64 {
    NET.try_with(Cell::get).unwrap_or(0)
}
