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
pub fn drive(program: &std::path::Path, arguments: &[OsString]) -> std::io::Result<i32> {
    let Some(runtime) = crate::worker_pool::runtime() else {
        return Err(std::io::Error::other("no runtime to drive the run on"));
    };
    runtime.block_on(async {
        let mut resumes = 0;
        loop {
            let status = child(program, arguments, resumes).await?;
            match next_after(status.code(), resumes) {
                Next::Resume => {
                    resumes += 1;
                    let _ = std::io::Write::write_all(
                        &mut crate::host::stderr(),
                        format!("cordial: taking the run up again in a new process ({resumes})\n")
                            .as_bytes(),
                    );
                }
                Next::End(code) => return Ok(code),
            }
        }
    })
}

/// One child, awaited.
async fn child(
    program: &std::path::Path,
    arguments: &[OsString],
    resumes: usize,
) -> std::io::Result<ExitStatus> {
    tokio::process::Command::new(program)
        .args(arguments)
        .env(RESUMABLE_VARIABLE, "1")
        .env(RESUMES_VARIABLE, resumes.to_string())
        .kill_on_drop(true)
        .spawn()?
        .wait()
        .await
}
