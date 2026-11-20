//! Per-test ownership of the shared Hotpath capture. Reporting stays on a helper
//! thread because Hotpath needs TLS that is unavailable during test TLS teardown.

use crate::profiling::{Capture, FinishReason, Identity};
use std::cell::RefCell;
use std::io;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread::JoinHandle;

struct Finish {
    done: mpsc::Sender<FinishReason>,
    helper: Option<JoinHandle<()>>,
}

impl Drop for Finish {
    fn drop(&mut self) {
        let reason = if std::thread::panicking() {
            FinishReason::Panic
        } else {
            FinishReason::Normal
        };
        let _ = self.done.send(reason);
        if let Some(helper) = self.helper.take()
            && helper.join().is_err()
        {
            eprintln!("cordial: the profile thread panicked; this test has no report");
        }
    }
}

thread_local! {
    static FINISH: RefCell<Option<Finish>> = const { RefCell::new(None) };
}

/// Test-thread owner. Declare before fixtures so its drop follows their teardown.
/// Inactive outside `CORDIAL_PERF_PROFILE_DIR` runs. Each measured test must run
/// in its own process (as nextest does); Hotpath cannot restart within libtest.
pub struct TestProfileGuard {
    active: bool,
    thread: PhantomData<Rc<()>>,
}

impl Drop for TestProfileGuard {
    fn drop(&mut self) {
        if self.active {
            FINISH.with(|slot| drop(slot.borrow_mut().take()));
        }
    }
}

/// Begin capture before test setup. Call before `init_tracing` or any fixtures.
pub fn profile_test() -> io::Result<TestProfileGuard> {
    let active = begin("test_entry")?;
    Ok(TestProfileGuard {
        active,
        thread: PhantomData,
    })
}

fn begin(boundary: &str) -> io::Result<bool> {
    let Some(root) = std::env::var_os("CORDIAL_PERF_PROFILE_DIR") else {
        return Ok(false);
    };
    let identity = Identity {
        role: "test".to_owned(),
        boundary: boundary.to_owned(),
        binary: Some(std::env::var("NEXTEST_BINARY_ID").map_err(io::Error::other)?),
        test: Some(std::env::var("NEXTEST_TEST_NAME").map_err(io::Error::other)?),
        repeat: std::env::var("CORDIAL_PROFILE_REPEAT")
            .ok()
            .and_then(|value| value.parse().ok()),
        ..Identity::default()
    };
    FINISH.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_some() {
            return Ok(false);
        }
        let (done, finished) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let helper = std::thread::Builder::new()
            .name("cordial-profile".to_owned())
            .spawn(move || {
                let mut capture = match Capture::start(&PathBuf::from(root), identity) {
                    Ok(capture) => capture,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                let _ = ready.send(Ok(()));
                let reason = finished.recv().unwrap_or(FinishReason::Panic);
                if let Err(error) = capture.finish(reason) {
                    eprintln!("cordial: could not finish test capture: {error}");
                }
            })?;
        match started.recv().map_err(io::Error::other)? {
            Ok(()) => {
                *slot = Some(Finish {
                    done,
                    helper: Some(helper),
                });
                Ok(true)
            }
            Err(error) => {
                let _ = helper.join();
                Err(error)
            }
        }
    })
}

/// Compatibility for existing tests. Its late boundary is recorded, never
/// represented as complete coverage of the test's setup.
pub(super) fn start() {
    if let Err(error) = begin("first_init_tracing") {
        eprintln!("cordial: could not start test capture: {error}");
    }
}
