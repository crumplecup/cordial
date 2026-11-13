//! The two ends of the session socket; they run commands, so they need the CLI.

mod client;
mod server;

pub use client::{build_environment, forward, socket_path, start_session, stop};
pub use server::serve;
