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
    verified_before: HashMap<&'static str, (u64, u64)>,
    durability_before: [u64; 4],
    keyed_before: (u64, u64),
    /// How many revisions had been started, which inputs had been written and how many times, and how many walks had
    /// found nothing changed and by query, when the region began.
    revisions_before: u64,
    writes_before: HashMap<&'static str, u64>,
    outcomes_before: (u64, u64),
    unchanged_before: HashMap<&'static str, u64>,
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
            verified_before: salsa::deep_verifications_by_ingredient(),
            durability_before: salsa::deep_verifications_by_durability(),
            keyed_before: salsa::keyed::counts(),
            revisions_before: salsa::revisions_started(),
            writes_before: salsa::input_writes(),
            outcomes_before: salsa::deep_verification_outcomes(),
            unchanged_before: salsa::deep_unchanged_by_ingredient(),
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

impl Segment {
    /// The queries whose memos this region re-validated most, with how many and how many dependencies they had.
    fn verified_top(&self) -> String {
        let mut more: Vec<(&'static str, u64, u64)> = salsa::deep_verifications_by_ingredient()
            .into_iter()
            .filter_map(|(name, (memos, edges))| {
                let before = self.verified_before.get(name).copied().unwrap_or_default();
                let memos = memos.saturating_sub(before.0);
                (memos > 0).then(|| (name, memos, edges.saturating_sub(before.1)))
            })
            .collect();
        more.sort_by_key(|(name, memos, _)| (std::cmp::Reverse(*memos), *name));
        more.iter()
            .take(TOP)
            .map(|(name, memos, edges)| format!("{name}={memos}/{edges}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Segment {
    /// What was written in this region, by input and how many times, most first: what started the revisions.
    fn writes_top(&self) -> String {
        let mut more: Vec<(&'static str, u64)> = salsa::input_writes()
            .into_iter()
            .filter_map(|(name, count)| {
                let more = count.saturating_sub(self.writes_before.get(name).copied().unwrap_or_default());
                (more > 0).then_some((name, more))
            })
            .collect();
        more.sort_by_key(|(name, count)| (std::cmp::Reverse(*count), *name));
        more.iter().take(TOP).map(|(name, count)| format!("{name}={count}")).collect::<Vec<_>>().join(" ")
    }

    /// The queries whose memos this region walked and found nothing changed in, most first: the walks that ran no
    /// query, which a revision that did not reach the memo costs.
    fn unchanged_top(&self) -> String {
        let mut more: Vec<(&'static str, u64)> = salsa::deep_unchanged_by_ingredient()
            .into_iter()
            .filter_map(|(name, count)| {
                let more = count.saturating_sub(self.unchanged_before.get(name).copied().unwrap_or_default());
                (more > 0).then_some((name, more))
            })
            .collect();
        more.sort_by_key(|(name, count)| (std::cmp::Reverse(*count), *name));
        more.iter().take(TOP).map(|(name, count)| format!("{name}={count}")).collect::<Vec<_>>().join(" ")
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
        let now = salsa::deep_verifications_by_durability();
        let durability: [u64; 4] = std::array::from_fn(|at| now[at].saturating_sub(self.durability_before[at]));
        let keyed = salsa::keyed::counts();
        let revisions = salsa::revisions_started();
        let outcomes = salsa::deep_verification_outcomes();
        let writes = self.writes_top();
        let writes_total: u64 = salsa::input_writes().values().sum::<u64>() - self.writes_before.values().sum::<u64>();
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
            verified_top = %self.verified_top(),
            verified_low = durability[0],
            verified_medium = durability[1],
            verified_high = durability[2],
            keyed_reads = keyed.0.saturating_sub(self.keyed_before.0),
            keyed_touches = keyed.1.saturating_sub(self.keyed_before.1),
            revisions = revisions.saturating_sub(self.revisions_before),
            at_revision = %revisions,
            writes = writes_total,
            writes_top = %writes,
            verified_unchanged = outcomes.0.saturating_sub(self.outcomes_before.0),
            verified_changed = outcomes.1.saturating_sub(self.outcomes_before.1),
            unchanged_top = %self.unchanged_top(),
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
