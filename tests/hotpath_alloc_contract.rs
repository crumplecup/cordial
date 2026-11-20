//! Controlled allocation fixtures for the locked Hotpath dependency. Each mode
//! gets a fresh process because Hotpath's counters and environment are global.
#![cfg(feature = "hotpath-alloc")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use miette::{IntoDiagnostic, Result};
use serde_json::Value;

#[hotpath::measure]
fn leaf() -> Vec<u8> {
    vec![3; 128]
}

#[hotpath::measure]
fn nested() -> (Vec<u8>, Vec<u8>) {
    let own = vec![5; 256];
    (own, leaf())
}

#[hotpath::measure]
fn resizing() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(32);
    bytes.resize(32, 1);
    bytes.reserve_exact(32);
    bytes.truncate(16);
    bytes.shrink_to_fit();
    bytes
}

#[hotpath::measure]
fn worker_allocation() -> Vec<u8> {
    vec![7; 192]
}

#[hotpath::measure]
fn worker_free(bytes: Vec<u8>) {
    drop(bytes);
}

/// A fault injector, not a global allocator or a measurement implementation.
/// Reallocation fails while ordinary allocation is delegated to System.
struct RejectRealloc;

// SAFETY: successful allocations and frees use System with unchanged layouts;
// null realloc leaves the original System block untouched.
unsafe impl GlobalAlloc for RejectRealloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies a valid allocation layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the block came from System with this layout.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, _: *mut u8, _: Layout, _: usize) -> *mut u8 {
        std::ptr::null_mut()
    }
}

#[hotpath::measure]
fn failed_reallocation() {
    let allocator = hotpath::CountingAllocator::with(RejectRealloc);
    let layout = Layout::array::<u8>(48).unwrap_or_else(|error| panic!("layout: {error}"));
    // SAFETY: nonzero layout, success checked, failed realloc keeps the old
    // allocation valid, and it is freed exactly once with its original layout.
    unsafe {
        let pointer = allocator.alloc(layout);
        assert!(!pointer.is_null());
        assert!(allocator.realloc(pointer, layout, 96).is_null());
        allocator.dealloc(pointer, layout);
    }
}

struct RejectAlloc;

// SAFETY: no allocation succeeds, so there is no block this allocator can free.
unsafe impl GlobalAlloc for RejectAlloc {
    unsafe fn alloc(&self, _: Layout) -> *mut u8 {
        std::ptr::null_mut()
    }

    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
}

#[hotpath::measure]
fn unsuccessful_requests() {
    let allocator = hotpath::CountingAllocator::with(RejectAlloc);
    let layout = Layout::array::<u8>(24).unwrap_or_else(|error| panic!("layout: {error}"));
    // SAFETY: valid nonzero layout. Both requests fail and return null, which is
    // checked and never dereferenced or freed.
    unsafe {
        assert!(allocator.alloc(layout).is_null());
        assert!(allocator.alloc_zeroed(layout).is_null());
    }
}

#[hotpath::measure]
fn short_lived() -> Vec<u8> {
    vec![19; 8]
}

#[hotpath::measure]
fn reserved_name_worker() -> Vec<u8> {
    vec![23; 72]
}

fn check_depth_limit() {
    // The locked dependency supports build-time depths up to 4096. Use heap
    // storage for guards so the check does not depend on the worker stack size.
    let outcome = std::panic::catch_unwind(|| {
        let mut guards = Vec::with_capacity(4097);
        for _ in 0..4097 {
            guards.push(hotpath::MeasurementGuardSync::new("depth", false, false));
        }
    });
    let panic = outcome.expect_err("exceeding the allocation depth must be visible");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or_default();
    assert!(message.contains("HOTPATH_ALLOC_MAX_DEPTH"), "{message}");
}

struct YieldOnce(bool);

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            Poll::Pending
        }
    }
}

#[hotpath::measure]
async fn migrating() -> (Vec<u8>, Vec<u8>) {
    let first = vec![11; 64];
    YieldOnce(false).await;
    (first, vec![13; 96])
}

#[hotpath::measure]
async fn cancelled() {
    let held = vec![17_u8; 80];
    let teardown = DropAllocation;
    std::future::pending::<()>().await;
    std::hint::black_box((held, teardown));
}

struct DropAllocation;

impl Drop for DropAllocation {
    #[hotpath::measure(impl_type = "DropAllocation")]
    fn drop(&mut self) {
        drop(std::hint::black_box(vec![29_u8; 40]));
    }
}

type PollOutcome<F> = (Pin<Box<F>>, Poll<<F as Future>::Output>);

fn poll_on_worker<F: Future + Send + 'static>(
    mut future: Pin<Box<F>>,
    name: &str,
) -> Result<PollOutcome<F>>
where
    F::Output: Send,
{
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let result = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()));
            (future, result)
        })
        .into_diagnostic()?
        .join()
        .map_err(|_| miette::miette!("allocation fixture worker panicked"))
}

fn row<'a>(report: &'a Value, section: &str, suffix: &str) -> &'a Value {
    report[section]["data"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|row| {
                row["name"]
                    .as_str()
                    .is_some_and(|name| name.ends_with(suffix))
            })
        })
        .unwrap_or_else(|| panic!("missing {suffix} in {section}: {report}"))
}

/// Run just this small test process in each mode, never a Cordial command or a
/// project workload. Subprocess isolation avoids mutating this test's environment.
#[test]
fn allocation_contract() -> Result<()> {
    for mode in [
        "exclusive",
        "inclusive",
        "count",
        "overflow",
        "internal",
        "depth",
    ] {
        let output = std::process::Command::new(std::env::current_exe().into_diagnostic()?)
            .args(["--exact", "capture_fixture", "--nocapture"])
            .env("CORDIAL_ALLOC_FIXTURE", mode)
            .env(
                "HOTPATH_ALLOC_METRIC",
                if mode == "count" { "count" } else { "bytes" },
            )
            .env(
                "HOTPATH_ALLOC_CUMULATIVE",
                if mode == "inclusive" { "1" } else { "0" },
            )
            .env(
                "HOTPATH_SHOW_INTERNAL_THREADS",
                if mode == "internal" { "1" } else { "0" },
            )
            .env_remove("HOTPATH_FOCUS")
            .env_remove("HOTPATH_REPORT")
            .env_remove("CORDIAL_PERF_PROFILE_DIR")
            .output()
            .into_diagnostic()?;
        assert!(
            output.status.success(),
            "{mode}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[test]
fn capture_fixture() -> Result<()> {
    let Ok(mode) = std::env::var("CORDIAL_ALLOC_FIXTURE") else {
        return Ok(());
    };
    let directory = tempfile::tempdir().into_diagnostic()?;
    let path = directory.path().join("fixture.json");
    assert!(
        cordial::live_bytes().is_some(),
        "link Cordial's shared allocator"
    );
    let guard = hotpath::HotpathGuardBuilder::new("fixture")
        .format(hotpath::Format::JsonPretty)
        .output_path(&path)
        .functions_limit(0)
        .threads_limit(0)
        .build();
    if mode == "depth" {
        check_depth_limit();
        drop(guard);
        return Ok(());
    }
    let retained = nested();
    let resized = resizing();
    failed_reallocation();
    unsuccessful_requests();
    let allocated = std::thread::Builder::new()
        .name("fixture-alloc".to_owned())
        .spawn(worker_allocation)
        .into_diagnostic()?
        .join()
        .map_err(|_| miette::miette!("allocation worker panicked"))?;
    std::thread::Builder::new()
        .name("fixture-free".to_owned())
        .spawn(move || worker_free(allocated))
        .into_diagnostic()?
        .join()
        .map_err(|_| miette::miette!("free worker panicked"))?;
    let (future, first) = poll_on_worker(Box::pin(migrating()), "fixture-poll-a")?;
    assert!(first.is_pending());
    let (future, second) = poll_on_worker(future, "fixture-poll-b")?;
    assert!(second.is_ready());
    drop(future);
    let (pending, result) = poll_on_worker(Box::pin(cancelled()), "fixture-cancel")?;
    assert!(result.is_pending());
    drop(pending);
    std::thread::Builder::new()
        .name("hp-fixture".to_owned())
        .spawn(|| drop(reserved_name_worker()))
        .into_diagnostic()?
        .join()
        .map_err(|_| miette::miette!("reserved-name worker panicked"))?;
    if mode == "overflow" {
        for index in 0..260 {
            std::thread::Builder::new()
                .name(format!("overflow-{index:03}"))
                .spawn(|| drop(short_lived()))
                .into_diagnostic()?
                .join()
                .map_err(|_| miette::miette!("short-lived worker panicked"))?;
        }
    }
    drop((retained, resized, second));
    drop(guard);

    let report: Value = serde_json::from_str(&std::fs::read_to_string(path).into_diagnostic()?)
        .into_diagnostic()?;
    let (leaf_total, nested_total, resize_total) = match mode.as_str() {
        "inclusive" => ("128 B", "384 B", "112 B"),
        "count" => ("1", "1", "3"),
        _ => ("128 B", "256 B", "112 B"),
    };
    assert_eq!(
        row(&report, "functions_alloc", "::leaf")["total"],
        leaf_total
    );
    assert_eq!(
        row(&report, "functions_alloc", "::nested")["total"],
        nested_total
    );
    assert_eq!(
        row(&report, "functions_alloc", "::resizing")["total"],
        resize_total
    );
    assert_eq!(
        row(&report, "functions_alloc", "::failed_reallocation")["total"],
        if mode == "count" { "1" } else { "48 B" }
    );
    // Capability gap in 0.31.2: alloc and alloc_zeroed record requests before
    // checking success. This is traffic, and cannot support a live-heap claim.
    assert_eq!(
        row(&report, "functions_alloc", "::unsuccessful_requests")["total"],
        if mode == "count" { "2" } else { "48 B" }
    );
    let freeing = row(&report, "threads", "fixture-free");
    assert!(
        hotpath::parse_bytes(freeing["dealloc_bytes"].as_str().unwrap_or_default())
            .is_some_and(|bytes| bytes >= 192)
    );
    assert_eq!(
        row(&report, "functions_alloc", "::worker_free")["total"],
        if mode == "count" { "0" } else { "0 B" }
    );
    // A future's poll bridge survives migration and records the allocations made
    // before cancellation. These are traffic totals, not retained heap.
    assert_eq!(
        row(&report, "functions_alloc", "::migrating")["total"],
        if mode == "count" { "2" } else { "160 B" }
    );
    assert_eq!(
        row(&report, "functions_alloc", "::cancelled")["total"],
        if mode == "count" { "1" } else { "80 B" }
    );
    // Cancellation destroys the future outside its poll scope. Its destructor
    // needs its own measurement; even inclusive async totals omit this work.
    assert_eq!(
        row(&report, "functions_alloc", "DropAllocation::drop")["total"],
        if mode == "count" { "1" } else { "40 B" }
    );
    let threads = report["threads"]["data"]
        .as_array()
        .unwrap_or_else(|| panic!("thread rows"));
    assert_eq!(
        threads.iter().any(|row| row["name"] == "hp-fixture"),
        mode == "internal",
        "the reserved hp- prefix hides application threads too"
    );
    // Name-based thread exclusion does not remove measured function traffic.
    assert_eq!(
        row(&report, "functions_alloc", "::reserved_name_worker")["total"],
        if mode == "count" { "1" } else { "72 B" }
    );
    if mode == "overflow" {
        assert_eq!(
            row(&report, "functions_alloc", "::short_lived")["calls"],
            260
        );
        let reported = threads
            .iter()
            .filter(|row| {
                row["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("overflow-"))
            })
            .count();
        assert!(
            reported < 260,
            "registry loses per-thread attribution after 256 lifetime slots"
        );
        assert!(reported > 0, "exited short-lived workers remain visible");
    }
    Ok(())
}
