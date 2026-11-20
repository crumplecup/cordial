// Original source: https://github.com/fornwall/allocation-counter
// Licensed under Apache-2.0 OR MIT. Adapted by Hotpath, then extended with an
// optional always-on live/peak meter; see ../PROVENANCE.md.

use std::alloc::{GlobalAlloc, Layout, System};
#[cfg(feature = "hotpath-live")]
use std::sync::atomic::{AtomicUsize, Ordering};

/// Allocator with optional function traffic (`hotpath-alloc`) and requested
/// live/peak bytes (`hotpath-live`). Without either feature it is a pass-through.
///
/// The live meter belongs to this allocator instance. Declare one instance as
/// the global allocator to measure the process's Rust heap. It includes recorder
/// allocations, unscoped allocations, and allocations before/after any profile
/// guard. Native allocations bypassing this allocator are outside its scope.
pub struct CountingAllocator<A = System> {
    inner: A,
    #[cfg(feature = "hotpath-live")]
    live: AtomicUsize,
    #[cfg(feature = "hotpath-live")]
    peak: AtomicUsize,
}

impl CountingAllocator<System> {
    pub const fn new() -> Self {
        Self::with(System)
    }
}

impl<A> CountingAllocator<A> {
    pub const fn with(inner: A) -> Self {
        Self {
            inner,
            #[cfg(feature = "hotpath-live")]
            live: AtomicUsize::new(0),
            #[cfg(feature = "hotpath-live")]
            peak: AtomicUsize::new(0),
        }
    }

    /// Exact requested bytes outstanding through this instance. Starts at zero
    /// on construction; there is no profile-dependent baseline or reset.
    ///
    /// Counter updates use atomics without allocating, locking, or accessing TLS.
    /// Successful allocations update before returning; frees update before
    /// forwarding to the inner allocator. Concurrent calls may be in flight.
    /// Allocator overhead, retention, and temporary storage inside realloc are
    /// not represented by requested layout sizes.
    #[cfg(feature = "hotpath-live")]
    pub fn live_bytes(&self) -> usize {
        self.live.load(Ordering::Relaxed)
    }

    /// Highest live-byte count since construction, with no reset. Realloc
    /// changes the live count by its size difference, avoiding a false peak from
    /// counting the old and new logical blocks simultaneously.
    ///
    /// A concurrent allocation can be between its live and peak updates; after
    /// allocator calls complete the peak is exact. This and `live_bytes()` are
    /// separate atomic reads, not a transactionally consistent snapshot.
    #[cfg(feature = "hotpath-live")]
    pub fn peak_live_bytes(&self) -> usize {
        self.peak.load(Ordering::Relaxed)
    }

    #[cfg(feature = "hotpath-live")]
    fn add_live(&self, size: usize) {
        let live = self.live.fetch_add(size, Ordering::Relaxed) + size;
        self.peak.fetch_max(live, Ordering::Relaxed);
    }

    #[cfg(feature = "hotpath-live")]
    fn remove_live(&self, size: usize) {
        self.live.fetch_sub(size, Ordering::Relaxed);
    }

    fn allocated(&self, size: usize) {
        #[cfg(feature = "hotpath-live")]
        self.add_live(size);
        #[cfg(feature = "hotpath-alloc")]
        crate::lib_on::functions::alloc::core::track_alloc(size);
        #[cfg(not(any(feature = "hotpath-live", feature = "hotpath-alloc")))]
        let _ = size;
    }
}

impl<A: Default> Default for CountingAllocator<A> {
    fn default() -> Self {
        Self::with(A::default())
    }
}

// SAFETY: pointers and layouts are forwarded unchanged to A. Tracking touches
// only counters and never the allocation; failed requests do not update them.
unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: caller supplies a valid layout, forwarded unchanged.
        let ptr = unsafe { self.inner.alloc(layout) };
        if !ptr.is_null() {
            self.allocated(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: caller supplies a valid layout, forwarded unchanged.
        let ptr = unsafe { self.inner.alloc_zeroed(layout) };
        if !ptr.is_null() {
            self.allocated(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        #[cfg(feature = "hotpath-live")]
        self.remove_live(layout.size());
        #[cfg(feature = "hotpath-alloc")]
        crate::lib_on::functions::alloc::core::track_dealloc(layout.size());
        // SAFETY: caller supplies a live block from this instance with its layout.
        unsafe { self.inner.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: caller supplies a live block, its layout, and valid new size.
        let new_ptr = unsafe { self.inner.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            #[cfg(feature = "hotpath-live")]
            if new_size >= layout.size() {
                self.add_live(new_size - layout.size());
            } else {
                self.remove_live(layout.size() - new_size);
            }
            // Traffic counts the full successful replacement, independent of
            // whether A moved the block. Live counts only the size difference.
            #[cfg(feature = "hotpath-alloc")]
            {
                crate::lib_on::functions::alloc::core::track_dealloc(layout.size());
                crate::lib_on::functions::alloc::core::track_alloc(new_size);
            }
        }
        new_ptr
    }
}
