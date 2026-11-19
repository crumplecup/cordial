//! A run in processes that are taken up again: the driver starts the run as a child process, and
//! when the child saves its state and exits because its resident memory neared the budget, starts
//! the next, which restores the state and goes on.
//!
//! This is what returns memory to the machine. The tasks of one process share one heap, and an
//! allocator may keep what they free, so the resident memory of a process that has run a while is
//! more than what it holds; a process that exits keeps nothing.
//!
//! The driver is a task of the worker runtime: it awaits the child's exit, and a child that was
//! stopped (by a signal, or by its own limit) ends the run with what it ended with.

use crate::{MEMORY_RESUME_EXIT, RESUMABLE_VARIABLE};
use std::ffi::OsString;
use std::process::ExitStatus;
use tracing::instrument;

/// Set by the driver to the number of processes that ran before a child: zero for the first.
pub const RESUMES_VARIABLE: &str = "CORDIAL_RESUMES";

/// Whether this process takes up a run that earlier processes began, so that what the first said of
/// the run is not said again.
#[must_use]
pub fn taken_up_again() -> bool {
    std::env::var(RESUMES_VARIABLE)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|resumes| resumes > 0)
}

/// The exit status of a run that stopped because taking it up again was getting it nowhere: the
/// states the processes saved did not grow, or a process could not restore the state it was to take
/// up. Another process would do the same.
pub const MEMORY_STUCK_EXIT: i32 = 77;

/// Set by the driver to a file that a process which saves its state to be taken up again writes the
/// size of the state to, so that the driver can tell whether the run is getting further.
pub const PROGRESS_VARIABLE: &str = "CORDIAL_RESUME_PROGRESS";

/// How many processes in a row may save a state that has not grown before the run is stopped.
pub const STALLS_ALLOWED: usize = 2;

/// How much a state has to grow over the one before to be progress: one part in twenty thousand, and
/// at least a kibibyte. The state is dominated by what every run has (the standard library's
/// analysis is most of a small workspace's), so what a process adds is a small share of it; a
/// process that did the same work again adds nothing.
fn progress_over(before: u64) -> u64 {
    (before / 20_000).max(1024)
}

/// Whether the run is stuck: the last [`STALLS_ALLOWED`] states grew by less than [`progress_over`]
/// the one before. `history` has the size of the state each process saved, in order.
/// A process that restores what the last saved and does the same work again saves the same state, so
/// a run that takes up again and again without getting further is seen after a few of them.
#[must_use]
pub fn stalled(history: &[u64]) -> bool {
    let stalls = history
        .windows(2)
        .rev()
        .take_while(|pair| pair[1] <= pair[0].saturating_add(progress_over(pair[0])))
        .count();
    stalls >= STALLS_ALLOWED
}

/// How a process that ends to be followed by another says how far the run has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// The size of the state it saved, in bytes: a state that did the same work again is the same size.
    StateBytes(u64),
    /// How many results the run has kept: each is one more, and counted exactly.
    Results(u64),
}

impl Progress {
    fn text(self) -> String {
        match self {
            Self::StateBytes(bytes) => format!("bytes {bytes}"),
            Self::Results(count) => format!("results {count}"),
        }
    }

    fn parse(text: &str) -> Option<Self> {
        let (kind, number) = text.trim().split_once(' ')?;
        let number = number.parse().ok()?;
        match kind {
            "bytes" => Some(Self::StateBytes(number)),
            "results" => Some(Self::Results(number)),
            _ => None,
        }
    }
}

/// Say, to the driver that will take the run up again, how far the run has got.
pub fn report_progress(progress: Progress) {
    if let Some(path) = std::env::var_os(PROGRESS_VARIABLE) {
        let _ = std::fs::write(path, progress.text());
    }
}

/// Whether the run is stuck, as [`stalled`] says of states, but by how many results the run has: the
/// last [`STALLS_ALLOWED`] processes each left no more than the one before.
#[must_use]
pub fn stalled_results(history: &[u64]) -> bool {
    let stalls = history
        .windows(2)
        .rev()
        .take_while(|pair| pair[1] <= pair[0])
        .count();
    stalls >= STALLS_ALLOWED
}

/// Whether the run is stuck, by whichever of the two the processes say how far they got in.
fn stuck(history: &[Progress]) -> bool {
    let bytes: Vec<u64> = history
        .iter()
        .filter_map(|progress| match progress {
            Progress::StateBytes(bytes) => Some(*bytes),
            Progress::Results(_) => None,
        })
        .collect();
    let results: Vec<u64> = history
        .iter()
        .filter_map(|progress| match progress {
            Progress::Results(count) => Some(*count),
            Progress::StateBytes(_) => None,
        })
        .collect();
    stalled(&bytes) || stalled_results(&results)
}

/// The most times a run is taken up again: a run that has still not finished after this many is
/// not one that more processes will finish.
pub const MOST_RESUMES: usize = 500;

/// What the driver does after a child ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Start another child: the last saved its state to be taken up again.
    Resume,
    /// The run is over, and ended with this status.
    End(i32),
}

/// What follows a child that ended with `code` (`None` if it was stopped by a signal), after
/// `resumes` children before it.
#[must_use]
pub fn next_after(code: Option<i32>, resumes: usize) -> Next {
    match code {
        Some(MEMORY_RESUME_EXIT) if resumes < MOST_RESUMES => Next::Resume,
        Some(MEMORY_RESUME_EXIT) => Next::End(1),
        Some(code) => Next::End(code),
        None => Next::End(1),
    }
}

/// Start `program` with `arguments` as a child that can be taken up again, and again while it says
/// it is to be, with the same arguments. Returns what the last ended with. The child's output is
/// this process's, so a person reads one run.
///
/// # Errors
///
/// When a child cannot be started or waited for.
#[instrument(level = "debug", skip(arguments), err(level = "warn"))]
pub fn drive(
    program: &std::path::Path,
    arguments: &[OsString],
    results: &std::path::Path,
) -> std::io::Result<i32> {
    let Some(runtime) = crate::worker_pool::runtime() else {
        return Err(std::io::Error::other("no runtime to drive the run on"));
    };
    let progress = std::env::temp_dir().join(format!("cordial-resume-{}", std::process::id()));
    let outcome = runtime.block_on(async {
        let mut resumes = 0;
        let mut history: Vec<Progress> = Vec::new();
        loop {
            let status = child(program, arguments, resumes, &progress, results).await?;
            match next_after(status.code(), resumes) {
                Next::Resume => {
                    if let Some(reported) = std::fs::read_to_string(&progress)
                        .ok()
                        .and_then(|text| Progress::parse(&text))
                    {
                        history.push(reported);
                    }
                    if stuck(&history) {
                        let _ = std::io::Write::write_all(
                            &mut crate::host::stderr(),
                            format!(
                                "cordial: stopped: {} processes in a row got the run no further \
                                 (what each reported, latest last: {:?}). The memory budget leaves \
                                 too little room to get further on this workspace: raise \
                                 --memory-target or --memory-limit.\n",
                                STALLS_ALLOWED + 1,
                                history.iter().rev().take(STALLS_ALLOWED + 1).rev().collect::<Vec<_>>()
                            )
                            .as_bytes(),
                        );
                        return Ok(MEMORY_STUCK_EXIT);
                    }
                    resumes += 1;
                    let _ = std::io::Write::write_all(
                        &mut crate::host::stderr(),
                        format!("cordial: taking the run up again in a new process ({resumes})\n")
                            .as_bytes(),
                    );
                }
                Next::End(code) => {
                    if code == 1 && resumes >= MOST_RESUMES {
                        let _ = std::io::Write::write_all(
                            &mut crate::host::stderr(),
                            format!("cordial: stopped: the run was taken up again {resumes} times and has still not finished.\n").as_bytes(),
                        );
                    }
                    return Ok(code);
                }
            }
        }
    });
    let _ = std::fs::remove_file(&progress);
    // What the processes handed on is of this run, and means nothing to another.
    let _ = std::fs::remove_file(results);
    outcome
}

/// One child, awaited.
async fn child(
    program: &std::path::Path,
    arguments: &[OsString],
    resumes: usize,
    progress: &std::path::Path,
    results: &std::path::Path,
) -> std::io::Result<ExitStatus> {
    tokio::process::Command::new(program)
        .args(arguments)
        .env(RESUMABLE_VARIABLE, "1")
        .env(RESUMES_VARIABLE, resumes.to_string())
        .env(PROGRESS_VARIABLE, progress)
        .env(crate::RESULT_STORE_VARIABLE, results)
        .kill_on_drop(true)
        .spawn()?
        .wait()
        .await
}
