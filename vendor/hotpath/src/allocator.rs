// Original source: https://github.com/fornwall/allocation-counter
// Licensed under Apache-2.0 OR MIT. Adapted by Hotpath, then extended with an
// optional live/peak and retained-origin tracking; see ../PROVENANCE.md.

use std::alloc::{GlobalAlloc, Layout, System};
#[cfg(feature = "hotpath-live")]
use std::sync::atomic::{AtomicUsize, Ordering};

/// Allocator with optional function traffic (`hotpath-alloc`) and requested
/// live/peak bytes (`hotpath-live`). `hotpath-retained` adds bounded retained
/// origins and private block prefixes. Without any of these it is a pass-through.
///
/// The live meter belongs to this allocator instance. Declare one instance as
/// the global allocator to measure the process's Rust heap. It includes recorder
/// allocations, unscoped allocations, and allocations before/after any profile
/// guard. Native allocations bypassing this allocator are outside its scope.
pub struct CountingAllocator<A = System> {
    inner: A,
    #[cfg(feature = "hotpath-retained")]
    origins: crate::retained::Registry,
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
            #[cfg(feature = "hotpath-retained")]
            origins: crate::retained::Registry::new(crate::MAX_ALLOCATION_ORIGINS),
            #[cfg(feature = "hotpath-live")]
            live: AtomicUsize::new(0),
            #[cfg(feature = "hotpath-live")]
            peak: AtomicUsize::new(0),
        }
    }

    /// Detailed-mode constructor with a smaller logical registry capacity.
    /// Storage remains fixed; capacity is clamped to MAX_ALLOCATION_ORIGINS.
    #[cfg(feature = "hotpath-retained")]
    pub const fn with_origin_capacity(inner: A, capacity: usize) -> Self {
        Self {
            inner,
            origins: crate::retained::Registry::new(capacity),
            live: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        }
    }

    /// Coherent requested-heap and retained-origin snapshot at a lock point.
    /// Rows reconcile internally during concurrent allocation. The cheap atomic
    /// meter reconciles with them once allocator calls quiesce. Snapshot storage
    /// is recorder work, and may itself appear in a global allocator snapshot.
    #[cfg(feature = "hotpath-retained")]
    pub fn heap_snapshot(&self) -> crate::HeapSnapshot {
        self.origins.snapshot()
    }

    /// Exact requested bytes outstanding through this instance. Starts at zero
    /// on construction; there is no profile-dependent baseline or reset.
    ///
    /// Live-only updates use atomics without allocation, locking or TLS.
    /// Retained mode also serializes registry updates; it never allocates metadata.
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

    #[cfg(all(feature = "hotpath-live", not(feature = "hotpath-retained")))]
    fn add_live(&self, size: usize) {
        let live = self.live.fetch_add(size, Ordering::Relaxed) + size;
        self.peak.fetch_max(live, Ordering::Relaxed);
    }

    #[cfg(all(feature = "hotpath-live", not(feature = "hotpath-retained")))]
    fn remove_live(&self, size: usize) {
        self.live.fetch_sub(size, Ordering::Relaxed);
    }

    fn allocated(&self, size: usize) {
        #[cfg(all(feature = "hotpath-live", not(feature = "hotpath-retained")))]
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

// SAFETY: each user layout is extended by a private prefix only in retained
// mode. The returned pointer preserves alignment and has the requested usable
// length. Matching free/realloc reconstructs the same backing layout, and no
// registry lock is held while calling the inner allocator.
unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: caller supplies a valid nonzero layout.
        unsafe { self.allocate(layout, false) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: caller supplies a valid nonzero layout; the entire backing
        // block is zeroed before writing only its private prefix.
        unsafe { self.allocate(layout, true) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        #[cfg(feature = "hotpath-retained")]
        let (ptr, backing) = {
            // SAFETY: a successful allocation had a valid extended layout.
            let (backing, offset) = unsafe { backing_layout(layout).unwrap_unchecked() };
            // SAFETY: ptr was returned at exactly offset into the backing block;
            // its aligned prefix contains the origin written by this instance.
            let base = unsafe { ptr.sub(offset) };
            let origin = unsafe { base.cast::<usize>().read() };
            self.origins
                .free(origin, layout.size(), offset, &self.live, &self.peak);
            (base, backing)
        };
        #[cfg(not(feature = "hotpath-retained"))]
        let backing = layout;
        #[cfg(all(feature = "hotpath-live", not(feature = "hotpath-retained")))]
        self.remove_live(layout.size());
        #[cfg(feature = "hotpath-alloc")]
        crate::lib_on::functions::alloc::core::track_dealloc(layout.size());
        // SAFETY: matching backing pointer/layout, still live in A.
        unsafe { self.inner.dealloc(ptr, backing) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        #[cfg(feature = "hotpath-retained")]
        let (base, backing, offset, origin, backing_size) = {
            // SAFETY: successful original allocation had a valid extension.
            let (backing, offset) = unsafe { backing_layout(layout).unwrap_unchecked() };
            let replacement = Layout::from_size_align(new_size, layout.align())
                .ok()
                .and_then(backing_layout);
            let Some((replacement, _)) = replacement else {
                self.origins.failed(true);
                return std::ptr::null_mut();
            };
            // SAFETY: valid original allocation with its private aligned prefix.
            let base = unsafe { ptr.sub(offset) };
            let origin = unsafe { base.cast::<usize>().read() };
            (base, backing, offset, origin, replacement.size())
        };
        #[cfg(not(feature = "hotpath-retained"))]
        let (base, backing, offset, backing_size) = (ptr, layout, 0, new_size);
        // SAFETY: valid original backing block and a valid nonzero new size.
        // Alignment is unchanged, hence the user offset also stays unchanged.
        let replacement = unsafe { self.inner.realloc(base, backing, backing_size) };
        if replacement.is_null() {
            #[cfg(feature = "hotpath-retained")]
            self.origins.failed(true);
            return replacement;
        }
        #[cfg(feature = "hotpath-retained")]
        self.origins
            .resize(origin, layout.size(), new_size, &self.live, &self.peak);
        #[cfg(all(feature = "hotpath-live", not(feature = "hotpath-retained")))]
        if new_size >= layout.size() {
            self.add_live(new_size - layout.size());
        } else {
            self.remove_live(layout.size() - new_size);
        }
        // Traffic counts a full replacement at the resizing operation. Retained
        // bytes remain attached to the original allocation, even after moving.
        #[cfg(feature = "hotpath-alloc")]
        {
            crate::lib_on::functions::alloc::core::track_dealloc(layout.size());
            crate::lib_on::functions::alloc::core::track_alloc(new_size);
        }
        // SAFETY: extended replacement holds offset plus new_size user bytes.
        unsafe { replacement.add(offset) }
    }
}

#[cfg(feature = "hotpath-retained")]
fn backing_layout(layout: Layout) -> Option<(Layout, usize)> {
    Layout::new::<usize>().extend(layout).ok()
}

impl<A: GlobalAlloc> CountingAllocator<A> {
    unsafe fn allocate(&self, layout: Layout, zeroed: bool) -> *mut u8 {
        #[cfg(feature = "hotpath-retained")]
        let (backing, offset) = match backing_layout(layout) {
            Some(pair) => pair,
            None => {
                self.origins.failed(false);
                return std::ptr::null_mut();
            }
        };
        #[cfg(not(feature = "hotpath-retained"))]
        let (backing, offset) = (layout, 0);
        // SAFETY: valid layout; zeroed requests preserve the inner guarantee.
        let base = unsafe {
            if zeroed {
                self.inner.alloc_zeroed(backing)
            } else {
                self.inner.alloc(backing)
            }
        };
        if base.is_null() {
            #[cfg(feature = "hotpath-retained")]
            self.origins.failed(false);
            return base;
        }
        #[cfg(feature = "hotpath-retained")]
        {
            let source = crate::lib_on::functions::alloc::core::current_origin();
            let origin =
                self.origins
                    .allocate(source, layout.size(), offset, &self.live, &self.peak);
            // SAFETY: backing begins with a valid, aligned usize prefix, outside
            // the user region. The same tag survives any successful realloc.
            unsafe { base.cast::<usize>().write(origin) };
        }
        self.allocated(layout.size());
        // SAFETY: offset plus the requested size fits this backing allocation.
        unsafe { base.add(offset) }
    }
}
