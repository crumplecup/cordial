//! Which terminal a command is running under, so a session can end with it.
//!
//! A shell is the leader of its terminal's process session, and a tool that runs
//! commands on someone's behalf puts each command in a session of its own with no
//! terminal. So the owner is found by walking up from the command to the nearest
//! process that has a controlling terminal, and taking that process's session
//! leader: the shell that owns the terminal.

use tracing::instrument;

/// One process as `/proc` describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessEntry {
    /// The process id.
    pub pid: u32,
    /// Its parent's process id.
    pub parent: u32,
    /// Its process session id, which is the pid of the session leader.
    pub session: u32,
    /// Its controlling terminal's device number, or 0 for none.
    pub terminal: u32,
}

/// Read `/proc/<pid>/stat`. The command name is in parentheses and may itself
/// contain spaces and parentheses, so the fields are counted from the last one.
#[instrument(level = "trace")]
pub fn read_entry(pid: u32) -> Option<ProcessEntry> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let mut fields = stat.rsplit_once(')')?.1.split_whitespace().skip(1);
    Some(ProcessEntry {
        pid,
        parent: fields.next()?.parse().ok()?,
        session: fields.nth(1)?.parse().ok()?,
        terminal: fields.next()?.parse().ok()?,
    })
}

/// The shell that owns the terminal a chain of processes, nearest first, runs
/// under: the session leader of the first one that has a terminal. `None` when no
/// process in the chain has one.
#[instrument(level = "debug", skip(chain))]
pub fn owner_of_chain(chain: &[ProcessEntry]) -> Option<u32> {
    chain
        .iter()
        .find(|entry| entry.terminal != 0)
        .map(|entry| entry.session)
}

/// This process and its ancestors, nearest first.
#[instrument(level = "debug")]
pub fn own_chain() -> Vec<ProcessEntry> {
    let mut chain = Vec::new();
    let mut pid = std::process::id();
    while pid > 1 {
        let Some(entry) = read_entry(pid) else {
            break;
        };
        pid = entry.parent;
        chain.push(entry);
    }
    chain
}

/// The shell of the terminal this command runs under, if it runs under one.
#[instrument(level = "debug")]
pub fn terminal_owner() -> Option<u32> {
    owner_of_chain(&own_chain())
}

/// Whether `owner`, the leader of its own session, is still running. A pid that
/// is gone, or reused by a process in another session, means the terminal ended.
#[instrument(level = "trace")]
pub fn leader_alive(owner: u32) -> bool {
    read_entry(owner).is_some_and(|entry| entry.session == owner)
}
