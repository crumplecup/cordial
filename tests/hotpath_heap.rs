//! Small deterministic fixtures for Hotpath's always-on heap meter. No project
//! workload or Cordial executable is run. Each meter wraps its own allocator so
//! test-runner allocations cannot affect exact assertions.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Barrier;
use std::sync::atomic::{AtomicPtr, Ordering};

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, 8).unwrap_or_else(|error| panic!("layout: {error}"))
}

#[test]
fn successful_allocations_and_zeroed_allocations_are_live_until_freed() {
    let allocator = hotpath::CountingAllocator::new();
    assert_eq!(allocator.live_bytes(), 0);
    assert_eq!(allocator.peak_live_bytes(), 0);
    // SAFETY: valid layouts, success checked before access/free, each block
    // freed exactly once by its originating allocator with its original layout.
    unsafe {
        let first = allocator.alloc(layout(32));
        assert!(!first.is_null());
        let zeroed = allocator.alloc_zeroed(layout(48));
        assert!(!zeroed.is_null());
        assert_eq!(std::slice::from_raw_parts(zeroed, 48), &[0; 48]);
        assert_eq!(allocator.live_bytes(), 80);
        assert_eq!(allocator.peak_live_bytes(), 80);
        allocator.dealloc(first, layout(32));
        assert_eq!(allocator.live_bytes(), 48);
        allocator.dealloc(zeroed, layout(48));
    }
    assert_eq!(allocator.live_bytes(), 0);
    assert_eq!(allocator.peak_live_bytes(), 80);
}

/// Deterministic in-place realloc backed by a larger System block. Layout
/// sizes exposed to the wrapper are logical sizes; backing capacity is fixed.
struct InPlace;

// SAFETY: all successful pointers have 256 bytes of storage aligned to 8; these
// fixtures only request alignment 8 and sizes <= 256. Free uses the actual
// backing layout. Failure preserves the original block and its contents.
unsafe impl GlobalAlloc for InPlace {
    unsafe fn alloc(&self, requested: Layout) -> *mut u8 {
        if requested.align() != 8 || requested.size() > 256 {
            return std::ptr::null_mut();
        }
        // SAFETY: valid backing layout, forwarded unchanged to System.
        unsafe { System.alloc(layout(256)) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, _: Layout) {
        // SAFETY: every successful allocation uses this backing layout.
        unsafe { System.dealloc(pointer, layout(256)) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, _: Layout, size: usize) -> *mut u8 {
        if size <= 256 {
            pointer
        } else {
            std::ptr::null_mut()
        }
    }
}

#[test]
fn inplace_realloc_counts_the_size_difference_and_failure_changes_nothing() {
    let allocator = hotpath::CountingAllocator::with(InPlace);
    // SAFETY: nonzero layouts supported by InPlace; success checked; realloc
    // preserves the live pointer; final free uses the last successful size.
    unsafe {
        assert!(allocator.alloc(layout(512)).is_null());
        assert!(allocator.alloc_zeroed(layout(512)).is_null());
        assert_eq!(allocator.live_bytes(), 0);
        assert_eq!(allocator.peak_live_bytes(), 0);
        #[cfg(feature = "hotpath-alloc")]
        let origin = hotpath::MeasurementGuardSync::new("original block", false, false);
        let first = allocator.alloc(layout(32));
        assert!(!first.is_null());
        #[cfg(feature = "hotpath-alloc")]
        drop(origin);
        first.write(73);
        let grown = allocator.realloc(first, layout(32), 64);
        assert_eq!(grown, first);
        assert_eq!(allocator.live_bytes(), 64);
        assert_eq!(allocator.peak_live_bytes(), 64);
        assert!(allocator.realloc(grown, layout(64), 512).is_null());
        assert_eq!(allocator.live_bytes(), 64);
        assert_eq!(allocator.peak_live_bytes(), 64);
        assert_eq!(grown.read(), 73);
        let shrunk = allocator.realloc(grown, layout(64), 16);
        assert_eq!(shrunk, first);
        assert_eq!(shrunk.read(), 73);
        assert_eq!(allocator.live_bytes(), 16);
        assert_eq!(allocator.peak_live_bytes(), 64);
        #[cfg(feature = "hotpath-alloc")]
        {
            let snapshot = allocator.heap_snapshot();
            let original = snapshot
                .origins
                .iter()
                .find(|r| r.name.as_deref() == Some("original block"))
                .unwrap_or_else(|| panic!("lost original allocation: {snapshot:?}"));
            assert_eq!(original.live_bytes, 16);
            assert_eq!(original.live_allocations, 1);
            assert_eq!(snapshot.origins[0].live_bytes, 0);
            assert_eq!(snapshot.peak_live_bytes, 64);
            assert_eq!(snapshot.live_header_bytes, std::mem::size_of::<usize>());
        }
        allocator.dealloc(shrunk, layout(16));
    }
    assert_eq!(allocator.live_bytes(), 0);
    #[cfg(feature = "hotpath-alloc")]
    {
        let snapshot = allocator.heap_snapshot();
        assert_eq!(snapshot.live_bytes, 0);
        assert_eq!(snapshot.peak_live_bytes, 64);
        assert_eq!(snapshot.allocation_failures, 2);
        assert_eq!(snapshot.reallocation_failures, 1);
        assert_eq!(snapshot.successful_allocations, 1);
        assert_eq!(snapshot.successful_reallocations, 2);
        assert_eq!(snapshot.allocated_bytes, 112);
        assert_eq!(snapshot.freed_bytes, 112);
        let peak = snapshot
            .peak
            .as_ref()
            .unwrap_or_else(|| panic!("missing peak"));
        assert_eq!(peak.live_bytes, 64);
        assert_eq!(peak.live_allocations, 1);
        assert_eq!(
            peak.event_sequence, 2,
            "failed requests and shrinking do not replace the peak"
        );
        assert_eq!(peak.origin_updates, 2);
    }
}

struct AlwaysMoves;

// SAFETY: delegates allocation/free to System with unchanged layouts. Realloc
// obtains new storage before freeing the old block, copies only the shared valid
// length, and leaves the old block untouched if allocation fails.
unsafe impl GlobalAlloc for AlwaysMoves {
    unsafe fn alloc(&self, requested: Layout) -> *mut u8 {
        // SAFETY: caller supplies a valid layout.
        unsafe { System.alloc(requested) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, requested: Layout) {
        // SAFETY: caller supplies a block from System with this layout.
        unsafe { System.dealloc(pointer, requested) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, size: usize) -> *mut u8 {
        let replacement = Layout::from_size_align(size, old.align())
            .unwrap_or_else(|error| panic!("layout: {error}"));
        // SAFETY: replacement layout is valid, caller supplies the old live
        // block; new storage is disjoint and copy length fits both blocks.
        unsafe {
            let moved = System.alloc(replacement);
            if !moved.is_null() {
                std::ptr::copy_nonoverlapping(pointer, moved, old.size().min(size));
                System.dealloc(pointer, old);
            }
            moved
        }
    }
}

#[test]
fn moved_realloc_preserves_contents_and_does_not_invent_a_live_peak() {
    let allocator = hotpath::CountingAllocator::with(AlwaysMoves);
    // SAFETY: nonzero layouts; only successful realloc pointers are used; final
    // free uses the most recent pointer/layout, exactly once.
    unsafe {
        #[cfg(feature = "hotpath-alloc")]
        let origin = hotpath::MeasurementGuardSync::new("original block", false, false);
        let first = allocator.alloc(layout(32));
        assert!(!first.is_null());
        #[cfg(feature = "hotpath-alloc")]
        drop(origin);
        first.write(61);
        let grown = allocator.realloc(first, layout(32), 64);
        assert!(!grown.is_null());
        assert_ne!(grown, first);
        assert_eq!(grown.read(), 61);
        assert_eq!(allocator.live_bytes(), 64);
        assert_eq!(allocator.peak_live_bytes(), 64);
        let shrunk = allocator.realloc(grown, layout(64), 16);
        assert!(!shrunk.is_null());
        assert_ne!(shrunk, grown);
        assert_eq!(shrunk.read(), 61);
        assert_eq!(allocator.live_bytes(), 16);
        assert_eq!(allocator.peak_live_bytes(), 64);
        #[cfg(feature = "hotpath-alloc")]
        {
            let snapshot = allocator.heap_snapshot();
            let original = snapshot
                .origins
                .iter()
                .find(|r| r.name.as_deref() == Some("original block"))
                .unwrap_or_else(|| panic!("lost original allocation: {snapshot:?}"));
            assert_eq!(original.live_bytes, 16);
            assert_eq!(original.live_allocations, 1);
            assert_eq!(snapshot.origins[0].live_bytes, 0);
            assert_eq!(snapshot.peak_live_bytes, 64);
            assert_eq!(snapshot.live_header_bytes, std::mem::size_of::<usize>());
        }
        allocator.dealloc(shrunk, layout(16));
    }
    assert_eq!(allocator.live_bytes(), 0);
}

#[test]
fn cross_thread_free_after_allocating_thread_exits() {
    let allocator = hotpath::CountingAllocator::new();
    let block = AtomicPtr::new(std::ptr::null_mut());
    std::thread::scope(|scope| {
        scope.spawn(|| {
            // SAFETY: valid nonzero layout. Ownership passes through AtomicPtr
            // to the freeing thread, after this worker has exited.
            let pointer = unsafe { allocator.alloc(layout(96)) };
            assert!(!pointer.is_null());
            block.store(pointer, Ordering::Release);
        });
    });
    assert_eq!(allocator.live_bytes(), 96);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let pointer = block.load(Ordering::Acquire);
            // SAFETY: the first worker's block remains live; matching allocator
            // and layout; only this worker frees it.
            unsafe { allocator.dealloc(pointer, layout(96)) };
        });
    });
    assert_eq!(allocator.live_bytes(), 0);
    assert_eq!(allocator.peak_live_bytes(), 96);
}

#[test]
fn concurrent_outstanding_blocks_reconcile_after_workers_finish() {
    let allocator = hotpath::CountingAllocator::new();
    let held = Barrier::new(5);
    let release = Barrier::new(5);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                // SAFETY: valid layout; block remains live until the release
                // barrier, then is freed once with its original layout.
                unsafe {
                    let pointer = allocator.alloc(layout(64));
                    assert!(!pointer.is_null());
                    held.wait();
                    release.wait();
                    allocator.dealloc(pointer, layout(64));
                }
            });
        }
        held.wait();
        assert_eq!(allocator.live_bytes(), 256);
        assert_eq!(allocator.peak_live_bytes(), 256);
        release.wait();
    });
    assert_eq!(allocator.live_bytes(), 0);
    assert_eq!(allocator.peak_live_bytes(), 256);
}

#[test]
fn instances_have_independent_baselines() {
    let first = hotpath::CountingAllocator::new();
    let second = hotpath::CountingAllocator::<System>::default();
    // SAFETY: valid nonzero layout, success checked, freed once by its origin.
    unsafe {
        let block = first.alloc(layout(24));
        assert!(!block.is_null());
        assert_eq!(first.live_bytes(), 24);
        assert_eq!(second.live_bytes(), 0);
        assert_eq!(second.peak_live_bytes(), 0);
        first.dealloc(block, layout(24));
    }
}
