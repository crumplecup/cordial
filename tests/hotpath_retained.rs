//! Small allocator-local fixtures for origin retention. The test runner's heap
//! cannot change these counters. No Cordial binary or project workload runs.
#![cfg(feature = "hotpath-alloc")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Barrier;
use std::sync::atomic::{AtomicPtr, Ordering};

use hotpath::{CountingAllocator, HeapSnapshot, OriginKind};
use miette::{IntoDiagnostic, Result};

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, 8).unwrap_or_else(|error| panic!("layout: {error}"))
}

fn allocate(allocator: &CountingAllocator, origin: &'static str, size: usize) -> *mut u8 {
    let guard = hotpath::MeasurementGuardSync::new(origin, false, false);
    // SAFETY: valid nonzero layout; caller assumes ownership and frees the block.
    let pointer = unsafe { allocator.alloc(layout(size)) };
    assert!(!pointer.is_null());
    drop(guard);
    pointer
}

fn origin<'a>(snapshot: &'a HeapSnapshot, name: &str) -> &'a hotpath::RetainedOrigin {
    snapshot
        .origins
        .iter()
        .find(|r| r.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("missing {name}: {snapshot:?}"))
}

fn reconciles(snapshot: &HeapSnapshot) {
    assert_eq!(
        snapshot.origins.iter().map(|r| r.live_bytes).sum::<usize>(),
        snapshot.live_bytes
    );
    assert_eq!(
        snapshot
            .origins
            .iter()
            .map(|r| r.live_allocations)
            .sum::<usize>(),
        snapshot.live_allocations
    );
    assert_eq!(
        snapshot.allocated_bytes - snapshot.freed_bytes,
        snapshot.live_bytes as u64
    );
}

#[test]
fn retained_return_values_keep_exclusive_origins_after_nested_scope_exit() {
    let allocator = CountingAllocator::new();
    let parent = hotpath::MeasurementGuardSync::new("retained parent", false, false);
    // SAFETY: valid layout, each successful pointer freed below with its layout.
    let first = unsafe { allocator.alloc(layout(32)) };
    assert!(!first.is_null());
    let second = allocate(&allocator, "retained child", 48);
    drop(parent);
    let snapshot = allocator.heap_snapshot();
    reconciles(&snapshot);
    assert_eq!(snapshot.live_bytes, 80);
    assert_eq!(origin(&snapshot, "retained parent").live_bytes, 32);
    assert_eq!(origin(&snapshot, "retained child").live_bytes, 48);
    assert_eq!(snapshot.live_header_bytes, 2 * std::mem::size_of::<usize>());
    assert!(snapshot.registry_bytes > 0);
    // SAFETY: both blocks are still live, allocated above with these layouts.
    unsafe {
        allocator.dealloc(first, layout(32));
        allocator.dealloc(second, layout(48));
    }
    let freed = allocator.heap_snapshot();
    reconciles(&freed);
    assert_eq!(freed.live_bytes, 0);
    assert_eq!(freed.live_header_bytes, 0);
    assert_eq!(freed.peak_live_bytes, 80);
    assert_eq!(
        origin(&freed, "retained child").id,
        origin(&snapshot, "retained child").id
    );
}

#[test]
fn cross_thread_resize_and_free_keep_the_exited_allocating_origin() {
    let allocator = CountingAllocator::new();
    let block = AtomicPtr::new(std::ptr::null_mut());
    std::thread::scope(|scope| {
        scope.spawn(|| {
            block.store(allocate(&allocator, "exited origin", 24), Ordering::Release);
        });
    });
    let before = allocator.heap_snapshot();
    assert_eq!(origin(&before, "exited origin").live_bytes, 24);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let guard = hotpath::MeasurementGuardSync::new("resizing thread", false, false);
            // SAFETY: live block transferred through AtomicPtr; success checked,
            // ownership retained for the final free on another thread below.
            let pointer =
                unsafe { allocator.realloc(block.load(Ordering::Acquire), layout(24), 56) };
            assert!(!pointer.is_null());
            block.store(pointer, Ordering::Release);
            drop(guard);
        });
    });
    let resized = allocator.heap_snapshot();
    reconciles(&resized);
    assert_eq!(origin(&resized, "exited origin").live_bytes, 56);
    assert_eq!(
        origin(&resized, "exited origin").id,
        origin(&before, "exited origin").id
    );
    assert!(
        !resized
            .origins
            .iter()
            .any(|r| r.name.as_deref() == Some("resizing thread"))
    );
    std::thread::scope(|scope| {
        scope.spawn(|| {
            // SAFETY: transferred live replacement, correct allocator and layout.
            unsafe {
                allocator.dealloc(block.load(Ordering::Acquire), layout(56));
            }
        });
    });
    let freed = allocator.heap_snapshot();
    reconciles(&freed);
    assert_eq!(freed.live_bytes, 0);
    assert_eq!(freed.successful_reallocations, 1);
    assert_eq!(freed.allocated_bytes, 80);
    assert_eq!(freed.freed_bytes, 80);
}

#[test]
fn overflow_retains_aggregate_bytes_and_frees_without_misattributing_origins() {
    let allocator = CountingAllocator::with_origin_capacity(System, 1);
    let first = allocate(&allocator, "one slot", 16);
    let overflow = allocate(&allocator, "overflowed origin", 40);
    let snapshot = allocator.heap_snapshot();
    reconciles(&snapshot);
    assert_eq!(snapshot.origin_capacity, 1);
    assert_eq!(snapshot.registered_origins, 1);
    assert_eq!(snapshot.overflow_allocations, 1);
    assert_eq!(snapshot.overflow_allocated_bytes, 40);
    let row = snapshot
        .origins
        .iter()
        .find(|r| r.kind == OriginKind::Overflow)
        .unwrap_or_else(|| panic!("missing overflow row"));
    assert_eq!(row.live_bytes, 40);
    assert_eq!(row.live_allocations, 1);
    // SAFETY: live blocks with matching allocator/layout, freed once each.
    unsafe {
        allocator.dealloc(first, layout(16));
        allocator.dealloc(overflow, layout(40));
    }
    let freed = allocator.heap_snapshot();
    reconciles(&freed);
    assert_eq!(freed.live_bytes, 0);
    assert_eq!(freed.overflow_allocations, 1);
}

#[test]
fn zeroed_aligned_unscoped_blocks_and_concurrent_snapshots_reconcile() {
    let allocator = CountingAllocator::new();
    let aligned =
        Layout::from_size_align(33, 4096).unwrap_or_else(|error| panic!("layout: {error}"));
    // SAFETY: valid aligned layout, success checked, matching free below.
    unsafe {
        let pointer = allocator.alloc_zeroed(aligned);
        assert!(!pointer.is_null());
        assert_eq!(pointer.addr() % 4096, 0);
        assert_eq!(std::slice::from_raw_parts(pointer, 33), &[0; 33]);
        let snapshot = allocator.heap_snapshot();
        reconciles(&snapshot);
        assert_eq!(snapshot.live_bytes, 33);
        assert_eq!(snapshot.live_header_bytes, 4096);
        assert_eq!(snapshot.origins[0].kind, OriginKind::Unscoped);
        assert_eq!(snapshot.origins[0].live_bytes, 33);
        allocator.dealloc(pointer, aligned);
    }
    let held = Barrier::new(5);
    let release = Barrier::new(5);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                let pointer = allocate(&allocator, "overlapping workers", 64);
                held.wait();
                release.wait();
                // SAFETY: this worker owns this successful allocation/layout.
                unsafe {
                    allocator.dealloc(pointer, layout(64));
                }
            });
        }
        held.wait();
        let snapshot = allocator.heap_snapshot();
        reconciles(&snapshot);
        assert_eq!(snapshot.live_bytes, 256);
        assert_eq!(snapshot.live_bytes, allocator.live_bytes());
        assert_eq!(snapshot.peak_live_bytes, allocator.peak_live_bytes());
        release.wait();
        // Exercise lock consistency while workers are freeing.
        for _ in 0..8 {
            reconciles(&allocator.heap_snapshot());
        }
    });
    assert_eq!(allocator.heap_snapshot().live_bytes, 0);
}

static REPORT_ALLOCATOR: CountingAllocator = CountingAllocator::new();
static RECORDER_BLOCK: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());

#[test]
fn report_carries_precapture_baseline_and_retained_origins_with_exact_roundtrip() -> Result<()> {
    let directory = tempfile::tempdir().into_diagnostic()?;
    let path = directory.path().join("retained.json");
    let before = allocate(&REPORT_ALLOCATOR, "before capture", 24);
    let guard = hotpath::HotpathGuardBuilder::new("retained report")
        .format(hotpath::Format::JsonPretty)
        .output_path(&path)
        .functions_limit(1)
        .threads_limit(1)
        .before_shutdown(|| {
            // Deliberate observer work in Hotpath's suspended recorder context.
            // SAFETY: valid layout; ownership passed to this test for cleanup.
            let pointer = unsafe { REPORT_ALLOCATOR.alloc(layout(16)) };
            assert!(!pointer.is_null());
            RECORDER_BLOCK.store(pointer, Ordering::Release);
        })
        .heap_provider(|| REPORT_ALLOCATOR.heap_snapshot())
        .build();
    let during = allocate(&REPORT_ALLOCATOR, "during capture", 48);
    drop(guard);
    let report: hotpath::json::JsonReport =
        serde_json::from_str(&std::fs::read_to_string(path).into_diagnostic()?)
            .into_diagnostic()?;
    let exact = report
        .allocation
        .unwrap_or_else(|| panic!("missing allocation section"));
    assert_eq!(
        exact.heap_status,
        hotpath::json::AllocationStatus::Available
    );
    let baseline = exact
        .heap_baseline
        .as_ref()
        .unwrap_or_else(|| panic!("missing baseline"));
    let final_heap = exact
        .heap
        .as_ref()
        .unwrap_or_else(|| panic!("missing heap"));
    reconciles(baseline);
    reconciles(final_heap);
    assert_eq!(baseline.live_bytes, 24);
    assert_eq!(final_heap.live_bytes, 88);
    assert_eq!(final_heap.origins[1].kind, OriginKind::Recorder);
    assert_eq!(final_heap.origins[1].live_bytes, 16);
    assert_eq!(origin(final_heap, "before capture").live_bytes, 24);
    assert_eq!(origin(final_heap, "during capture").live_bytes, 48);
    let json = serde_json::to_string(&exact).into_diagnostic()?;
    assert_eq!(
        serde_json::from_str::<hotpath::json::AllocationReport>(&json).into_diagnostic()?,
        exact
    );
    // Exact rows are independent of display truncation.
    assert!(exact.functions.as_ref().is_some_and(|rows| rows.len() > 1));
    // SAFETY: both blocks retained across report construction, matching layouts.
    unsafe {
        REPORT_ALLOCATOR.dealloc(before, layout(24));
        REPORT_ALLOCATOR.dealloc(during, layout(48));
        REPORT_ALLOCATOR.dealloc(
            RECORDER_BLOCK.swap(std::ptr::null_mut(), Ordering::AcqRel),
            layout(16),
        );
    }
    assert_eq!(REPORT_ALLOCATOR.heap_snapshot().live_bytes, 0);
    Ok(())
}

struct YieldOnce(bool);
impl std::future::Future for YieldOnce {
    type Output = ();
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0 {
            std::task::Poll::Ready(())
        } else {
            self.0 = true;
            std::task::Poll::Pending
        }
    }
}

#[hotpath::measure]
async fn migrating_origins(
    allocator: std::sync::Arc<CountingAllocator>,
    blocks: std::sync::Arc<[AtomicPtr<u8>; 2]>,
) {
    // SAFETY: valid layout; ownership passes to the caller through AtomicPtr.
    blocks[0].store(unsafe { allocator.alloc(layout(64)) }, Ordering::Release);
    YieldOnce(false).await;
    // SAFETY: valid layout; ownership passes to the caller through AtomicPtr.
    blocks[1].store(unsafe { allocator.alloc(layout(96)) }, Ordering::Release);
}

fn poll_elsewhere<F: std::future::Future + Send + 'static>(
    mut future: std::pin::Pin<Box<F>>,
) -> std::pin::Pin<Box<F>> {
    std::thread::spawn(move || {
        let _ = future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()));
        future
    })
    .join()
    .unwrap_or_else(|_| panic!("poll worker panicked"))
}

#[test]
fn migrating_polls_keep_one_origin_and_cancellation_keeps_retained_blocks_visible() {
    let allocator = std::sync::Arc::new(CountingAllocator::new());
    let blocks = std::sync::Arc::new([
        AtomicPtr::new(std::ptr::null_mut()),
        AtomicPtr::new(std::ptr::null_mut()),
    ]);
    let future = poll_elsewhere(Box::pin(migrating_origins(
        allocator.clone(),
        blocks.clone(),
    )));
    let pending = allocator.heap_snapshot();
    let name = pending
        .origins
        .iter()
        .find_map(|r| {
            r.name
                .as_deref()
                .filter(|name| name.ends_with("::migrating_origins"))
        })
        .unwrap_or_else(|| panic!("missing async origin: {pending:?}"))
        .to_owned();
    assert_eq!(origin(&pending, &name).live_bytes, 64);
    let future = poll_elsewhere(future);
    drop(future);
    let ready = allocator.heap_snapshot();
    reconciles(&ready);
    assert_eq!(origin(&ready, &name).live_bytes, 160);
    assert_eq!(origin(&pending, &name).id, origin(&ready, &name).id);
    for (i, size) in [64, 96].into_iter().enumerate() {
        let pointer = blocks[i].swap(std::ptr::null_mut(), Ordering::AcqRel);
        assert!(!pointer.is_null());
        // SAFETY: successful block transferred from its poll; matching layout.
        unsafe {
            allocator.dealloc(pointer, layout(size));
        }
    }
    let pending = poll_elsewhere(Box::pin(migrating_origins(
        allocator.clone(),
        blocks.clone(),
    )));
    drop(pending); // cancellation does not magically free buffers returned via AtomicPtr
    let cancelled = allocator.heap_snapshot();
    reconciles(&cancelled);
    assert_eq!(origin(&cancelled, &name).live_bytes, 64);
    let pointer = blocks[0].swap(std::ptr::null_mut(), Ordering::AcqRel);
    assert!(!pointer.is_null());
    // SAFETY: pending poll's successful allocation, still held by the caller.
    unsafe {
        allocator.dealloc(pointer, layout(64));
    }
    assert_eq!(allocator.heap_snapshot().live_bytes, 0);
}

#[hotpath::measure]
async fn unwinding_poll() {
    std::panic::panic_any("controlled poll unwind");
}

#[test]
fn unwinding_poll_restores_the_origin_stack() {
    use std::future::Future;
    let allocator = CountingAllocator::new();
    let mut future = Box::pin(unwinding_poll());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
    }));
    assert!(result.is_err());
    drop(future);
    // SAFETY: valid layout; matched free below. With no active poll/guard this
    // must be unscoped, rather than attributed to the panicking future.
    let pointer = unsafe { allocator.alloc(layout(24)) };
    assert!(!pointer.is_null());
    let snapshot = allocator.heap_snapshot();
    assert_eq!(snapshot.origins[0].live_bytes, 24);
    assert_eq!(snapshot.registered_origins, 0);
    // SAFETY: still-live block, correct allocator and layout.
    unsafe {
        allocator.dealloc(pointer, layout(24));
    }
}

#[test]
fn snapshots_during_concurrent_allocation_churn_are_coherent() {
    let allocator = CountingAllocator::new();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..64 {
                    let pointer = allocate(&allocator, "churning worker", 32);
                    // SAFETY: this worker owns this allocation with this layout.
                    unsafe {
                        allocator.dealloc(pointer, layout(32));
                    }
                }
            });
        }
        for _ in 0..32 {
            reconciles(&allocator.heap_snapshot());
        }
    });
    let snapshot = allocator.heap_snapshot();
    reconciles(&snapshot);
    assert_eq!(snapshot.live_bytes, 0);
    assert_eq!(snapshot.successful_allocations, 256);
    assert_eq!(snapshot.deallocations, 256);
    assert_eq!(snapshot.peak_live_bytes, allocator.peak_live_bytes());
}
