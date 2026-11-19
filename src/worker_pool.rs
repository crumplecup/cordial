//! The concurrency of a pass over thousands of functions, managed by one Tokio runtime that lives as
//! long as the process.
//!
//! - **Tasks.** Every item of a round is a task of a `JoinSet`, so a round is one group that is
//!   joined as a whole and whose panics are collected, not a count of threads that were started.
//! - **A gate.** A `Semaphore` says how many items run at once. It is read by every item as it
//!   starts, so the pressure ladder can narrow or widen it while a round is running, and not only
//!   between the chunks of a pass (see [`allow`]).
//! - **Blocking work in a task.** The work is heavy and does not wait (queries recurse deeply and
//!   hold what they compute), so an item runs inside its own task on the runtime's workers, in
//!   `block_in_place`, which tells the runtime that the worker is busy so that it moves the worker's
//!   other tasks elsewhere. No item goes to a thread of its own: the runtime has one worker and
//!   hands its role to one of at most as many threads as there are cores, which it keeps, so every
//!   round of every pass runs on the same few threads,
//!   which matters to a profile that follows threads one by one (`hotpath-alloc` has 256 slots and
//!   never reclaims one).
//! - **Metrics.** The runtime's own counts of its tasks and its queue are said after every round.
//!
//! A task owns what it uses (`'static`): the items are shared by `Arc`, and each running item is
//! lent one of the resources the caller gave the round (a handle onto the database, say), which goes
//! back when the item is done and is dropped, with the rest of the round, before the round returns.
//! So nothing is held between rounds, which is what lets the caller take the database exclusively
//! between them.

use std::any::Any;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;
use tracing::instrument;

/// Stack for a worker: inferring a body recurses deeply, and a spawned thread's default stack is far
/// smaller than the main thread's.
const STACK_BYTES: usize = 64 * 1024 * 1024;

/// How long an idle thread of the pool is kept: the run, for all there is to it.
const KEEP_IDLE: Duration = Duration::from_secs(60 * 60);

/// How many items run at once at most: the cores there are.
#[must_use]
pub fn pool_size() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

static RUNTIME: OnceLock<Option<Runtime>> = OnceLock::new();

fn runtime() -> Option<&'static Runtime> {
    RUNTIME
        .get_or_init(|| {
            let built = Builder::new_multi_thread()
                .worker_threads(1)
                .max_blocking_threads(pool_size())
                .thread_name("cordial-prime")
                .thread_stack_size(STACK_BYTES)
                .thread_keep_alive(KEEP_IDLE)
                .build();
            match built {
                Ok(runtime) => Some(runtime),
                Err(error) => {
                    tracing::warn!(error = ?error, "could not start the worker runtime");
                    None
                }
            }
        })
        .as_ref()
}

/// How many items may run at once, and what is owed to narrowing it while some are running.
struct Gate {
    permits: Arc<Semaphore>,
    allowed: Mutex<usize>,
    /// Permits that were to be taken away but were held: each is taken as it is given back.
    owed: AtomicUsize,
}

fn gate() -> &'static Gate {
    static GATE: OnceLock<Gate> = OnceLock::new();
    GATE.get_or_init(|| Gate {
        permits: Arc::new(Semaphore::new(pool_size())),
        allowed: Mutex::new(pool_size()),
        owed: AtomicUsize::new(0),
    })
}

/// Allow `workers` items to run at once (at least one, and not more than [`pool_size`]). It takes
/// effect at once for the items that have not started, and for those that have when they finish.
#[instrument(level = "debug")]
pub fn allow(workers: usize) {
    let gate = gate();
    let wanted = workers.clamp(1, pool_size());
    let Ok(mut allowed) = gate.allowed.lock() else {
        return;
    };
    let before = *allowed;
    *allowed = wanted;
    if wanted > before {
        let mut more = wanted - before;
        // What was owed is no longer: the permits that were to go stay.
        let _ = gate
            .owed
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |owed| {
                let cancelled = owed.min(more);
                more -= cancelled;
                Some(owed - cancelled)
            });
        gate.permits.add_permits(more);
    } else if wanted < before {
        let fewer = before - wanted;
        let taken = gate.permits.forget_permits(fewer);
        gate.owed.fetch_add(fewer - taken, Ordering::SeqCst);
    }
}

/// How many items may run at once now.
#[must_use]
pub fn allowed() -> usize {
    gate()
        .allowed
        .lock()
        .map_or(pool_size(), |allowed| *allowed)
}

/// Give a permit back, or let it go if the gate was narrowed while it was held.
fn release(permit: OwnedSemaphorePermit) {
    let owed = gate()
        .owed
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |owed| {
            owed.checked_sub(1)
        });
    if owed.is_ok() {
        permit.forget();
    }
}

/// What every task of one round shares.
struct Round<T, R, W, S> {
    items: Arc<[T]>,
    start: usize,
    length: usize,
    /// How many items have been taken.
    next: AtomicUsize,
    resources: Mutex<Vec<R>>,
    work: W,
    stop: S,
    /// Whether an item has panicked, so that the ones that have not started do not.
    panicked: AtomicBool,
    first_panic: Mutex<Option<Box<dyn Any + Send>>>,
}

impl<T, R, W, S> Round<T, R, W, S>
where
    T: Send + Sync + 'static,
    R: Send + 'static,
    W: Fn(&R, &T) + Send + Sync + 'static,
    S: Fn(usize) -> bool + Send + Sync + 'static,
{
    /// One task: wait for the gate, take the next item (items are taken in order, so the ones taken
    /// are always the first of the range), run it on the blocking pool with a resource, and give
    /// both back.
    async fn item(self: Arc<Self>, dispatch: tracing::Dispatch) {
        let Ok(permit) = Arc::clone(&gate().permits).acquire_owned().await else {
            return;
        };
        // Take the next item, once, whichever of the tasks asks: the number that were taken is what
        // `stop` is asked about, and the taken are always the first of the range.
        let taken = loop {
            let now = self.next.load(Ordering::SeqCst);
            if self.panicked.load(Ordering::SeqCst) || now >= self.length || (self.stop)(now) {
                release(permit);
                return;
            }
            if self
                .next
                .compare_exchange(now, now + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                break now;
            }
        };
        let resource = self.resources.lock().ok().and_then(|mut all| all.pop());
        let Some(resource) = resource else {
            // More items running than there are resources cannot be: the gate is never wider than
            // the cores, and a round has a resource for each.
            release(permit);
            return;
        };
        // The item runs in this task, on the runtime's worker. It does not yield (a query is a long
        // computation and not a wait), so the runtime is told, and hands the rest of this worker's
        // tasks to another while it runs.
        let outcome = tokio::task::block_in_place(|| {
            tracing::dispatcher::with_default(&dispatch, || {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    (self.work)(&resource, &self.items[self.start + taken]);
                }))
            })
        });
        if let Ok(mut all) = self.resources.lock() {
            all.push(resource);
        }
        if let Err(payload) = outcome {
            self.panicked.store(true, Ordering::SeqCst);
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied());
            // A panic that carries a message says what went wrong; Salsa's own unwinding (a
            // cancellation) carries none.
            tracing::error!(
                target: "cordial::prime",
                message = message.unwrap_or("(an unwind with no message)"),
                "a worker panicked"
            );
            if let Ok(mut first) = self.first_panic.lock() {
                first.get_or_insert(payload);
            }
        }
        release(permit);
    }
}

/// Run `work` over `items[range]`, each once, with at most [`allowed`] running at once, and wait
/// until the round is over. Each running item is lent one of `resources`. Returns how many items it
/// ran: all of them, unless `stop` says to (it is asked, before each item is taken, how many have
/// been), when the ones that were not taken are the last of the range.
///
/// Without a runtime the items run one after another on this thread.
///
/// # Panics
///
/// Resumes the panic of an item that panicked, after the ones that had started are done.
#[instrument(level = "debug", skip_all, fields(items = range.len()))]
pub fn run_items<T, R, W, S>(
    items: &Arc<[T]>,
    range: Range<usize>,
    resources: Vec<R>,
    work: W,
    stop: S,
) -> usize
where
    T: Send + Sync + 'static,
    R: Send + 'static,
    W: Fn(&R, &T) + Send + Sync + 'static,
    S: Fn(usize) -> bool + Send + Sync + 'static,
{
    let length = range.len();
    let round = Arc::new(Round {
        items: Arc::clone(items),
        start: range.start,
        length,
        next: AtomicUsize::new(0),
        resources: Mutex::new(resources),
        work,
        stop,
        panicked: AtomicBool::new(false),
        first_panic: Mutex::new(None),
    });
    // A subscriber set for one thread would not see the workers' events.
    let dispatch = tracing::dispatcher::get_default(Clone::clone);
    let Some(runtime) = runtime() else {
        let resource = round.resources.lock().ok().and_then(|mut all| all.pop());
        if let Some(resource) = resource {
            while !(round.stop)(round.next.load(Ordering::SeqCst))
                && round.next.load(Ordering::SeqCst) < length
            {
                let index = round.next.fetch_add(1, Ordering::SeqCst);
                (round.work)(&resource, &round.items[round.start + index]);
            }
        }
        return round.next.load(Ordering::SeqCst).min(length);
    };
    runtime.block_on(async {
        let mut tasks = JoinSet::new();
        for _ in 0..length {
            tasks.spawn(Arc::clone(&round).item(dispatch.clone()));
        }
        while let Some(joined) = tasks.join_next().await {
            if let Err(error) = joined
                && error.is_panic()
                && let Ok(mut first) = round.first_panic.lock()
            {
                first.get_or_insert(error.into_panic());
            }
        }
    });
    let metrics = runtime.metrics();
    tracing::debug!(
        target: "cordial::pool",
        alive_tasks = metrics.num_alive_tasks(),
        queued = metrics.global_queue_depth(),
        allowed = allowed(),
        "the worker pool after a round"
    );
    let first = round
        .first_panic
        .lock()
        .ok()
        .and_then(|mut first| first.take());
    if let Some(payload) = first {
        std::panic::resume_unwind(payload);
    }
    round.next.load(Ordering::SeqCst).min(length)
}
