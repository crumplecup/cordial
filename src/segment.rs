//! Timed regions of a run, with what each one cost in work, so that a slow region is found by reading and not by
//! adding an event to it.
//!
//! A [`Segment`] is begun where a region starts and says, when it is dropped, how long the region took, how many
//! queries Salsa executed in it, how many memos it re-validated and how many edges that walked, which queries ran
//! most, and the scope (the package being analysed) and phase it was in. A segment begun inside another has the
//! other's name as the front of its path, so `flips/finish/settle/bodies` is the bodies step of the settle in the
//! finish of the flips. The event is `cordial::segment`: the run's metrics add up each path, so two runs (flips and
//! copies, or this commit and the one before) are compared by `cordial perf compare` path by path.
//!
//! Names are lower case words only: a metric's name drops digits and joins words with `_`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use tracing::instrument;

use crate::LiveStatus;

thread_local! {
    /// The segments begun on this thread and not yet dropped, outermost first.
    static OPEN: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    /// What the thread is analysing, said in every segment event.
    static SCOPE: RefCell<String> = const { RefCell::new(String::new()) };
}

/// How many of the queries that ran most a segment names.
const TOP: usize = 6;

/// A region of a run that says what it cost when it ends.
#[must_use = "a segment measures from where it is begun to where it is dropped"]
pub struct Segment {
    path: String,
    started: Instant,
    executed: u64,
    verified: (u64, u64),
    before: HashMap<String, u64>,
}

impl Segment {
    /// Begin a region called `name`, inside the one begun last on this thread, if there is one.
    #[instrument(level = "trace")]
    pub fn begin(name: &'static str) -> Self {
        let path = OPEN.with(|open| {
            let mut open = open.borrow_mut();
            open.push(name);
            open.join("/")
        });
        let live = LiveStatus::global();
        Self {
            path,
            started: Instant::now(),
            executed: live.executed(),
            verified: salsa::deep_verifications(),
            before: live.counts(),
        }
    }

    /// Say what the thread is analysing, until the guard is dropped: the package, so that a segment that runs for
    /// each can be told apart.
    #[instrument(level = "trace")]
    pub fn scope(name: &str) -> ScopeGuard {
        let before = SCOPE.with(|scope| std::mem::replace(&mut *scope.borrow_mut(), name.to_owned()));
        ScopeGuard { before }
    }
}

impl Drop for Segment {
    fn drop(&mut self) {
        OPEN.with(|open| {
            open.borrow_mut().pop();
        });
        let live = LiveStatus::global();
        let verified = salsa::deep_verifications();
        let scope = SCOPE.with(|scope| scope.borrow().clone());
        tracing::info!(
            target: "cordial::segment",
            label = %self.path,
            scope = %scope,
            phase = %live.phase(),
            micros = u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX),
            queries = live.executed().saturating_sub(self.executed),
            verified_memos = verified.0.saturating_sub(self.verified.0),
            verified_edges = verified.1.saturating_sub(self.verified.1),
            top = %live.top_since(&self.before, TOP),
            "a region of the run"
        );
    }
}

/// Puts back what [`Segment::scope`] replaced.
pub struct ScopeGuard {
    before: String,
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        SCOPE.with(|scope| *scope.borrow_mut() = std::mem::take(&mut self.before));
    }
}
