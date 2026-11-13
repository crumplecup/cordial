//! A long-lived session: one process that keeps the loaded database between
//! commands, so an edit costs what it invalidates instead of a reload. See
//! `docs/planning/incremental-cache.md`.

mod databases;
mod output;
mod protocol;
#[cfg(feature = "cli")]
mod transport;

pub use databases::{keep, keep_between_runs, keeping, key_of, take_current};
pub use output::{FrameSink, Out, Stream, stderr, stdout, with_sink};
pub use protocol::{Frame, PROTOCOL, Request, read_frame, write_frame};
#[cfg(feature = "cli")]
pub use transport::{
    ProcessEntry, build_environment, forward, owner_of_chain, serve, socket_path, start_session,
    stop, terminal_owner,
};
