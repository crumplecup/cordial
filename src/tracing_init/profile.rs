//! Per-test profiling for `cordial perf tests profile`.
//!
//! When `CORDIAL_PERF_PROFILE_DIR` is set, a test process starts a `hotpath`
//! guard at the first `init_tracing` and the guard writes its report, named from
//! nextest's `NEXTEST_BINARY_ID` and `NEXTEST_TEST_NAME`, when the test ends.
//! Nothing runs without the variable, so ordinary test runs are untouched.
//!
//! The guard lives on a helper thread. Its report is written when it drops, and
//! that cannot happen inside the test thread's own thread-local destruction (the
//! report needs thread-local state that is already gone). The test thread holds
//! a [`Finish`] instead, whose destructor tells the helper to drop the guard and
//! waits until the report is on disk.

use crate::sync::mpsc;
use std::cell::RefCell;
use std::path::PathBuf;
use std::thread::JoinHandle;

/// Held by the test thread; ending the thread finishes the report.
struct Finish {
    done: Option<mpsc::Sender<()>>,
    helper: Option<JoinHandle<()>>,
}

impl Drop for Finish {
    #[tracing::instrument(level = "trace", skip(self))]
    fn drop(&mut self) {
        drop(self.done.take());
        if let Some(helper) = self.helper.take()
            && helper.join().is_err()
        {
            tracing::warn!("the profile thread panicked; this test has no report");
        }
    }
}

thread_local! {
    static FINISH: RefCell<Option<Finish>> = const { RefCell::new(None) };
}

/// The file a test's report goes to, or `None` when this is not a profile run.
#[tracing::instrument(level = "debug")]
#[hotpath::measure]
fn report_path() -> Option<PathBuf> {
    let dir = std::env::var_os("CORDIAL_PERF_PROFILE_DIR")?;
    let binary = std::env::var("NEXTEST_BINARY_ID").ok()?;
    let test = std::env::var("NEXTEST_TEST_NAME").ok()?;
    let name = format!("{binary}${test}.json").replace('/', "_");
    Some(PathBuf::from(dir).join(name))
}

/// Start profiling this test if the run asked for it.
#[tracing::instrument(level = "debug")]
#[hotpath::measure]
pub(super) fn start() {
    let Some(path) = report_path() else {
        return;
    };
    FINISH.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_some() {
            return;
        }
        let (done, finished) = crate::sync::channel!((), "profile-done");
        let (ready, started) = crate::sync::channel!((), "profile-ready");
        let spawned = std::thread::Builder::new()
            .name("cordial-profile".to_owned())
            .spawn(move || {
                let _guard = crate::profiling::guard("test", path);
                let _ = ready.send(());
                // Waits for the sender to drop, which is the test ending.
                let _ = finished.recv();
                // `_guard` drops here, which writes the report.
            });
        match spawned {
            Ok(helper) => {
                let _ = started.recv();
                *slot = Some(Finish {
                    done: Some(done),
                    helper: Some(helper),
                });
            }
            Err(error) => tracing::warn!(%error, "could not start the profile thread"),
        }
    });
}
