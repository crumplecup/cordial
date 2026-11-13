//! `cordial session`: keep one process holding the loaded workspace.

use clap::Subcommand;
use std::io::Write;
use tracing::instrument;

use crate::CordialResult;
use crate::host::{self, socket_path};

use super::commands::ActCtx;

/// Subcommands of `cordial session`.
#[derive(Subcommand)]
pub enum SessionCommands {
    /// Start a fresh session now, replacing one that is running. `open` starts
    /// one by itself when none is.
    Start,
    /// Ask this project's session to exit.
    Stop,
    /// Say whether this project has a session running.
    Status,
    /// Run the session in this process (what `start` launches).
    Serve,
}

impl SessionCommands {
    #[instrument(level = "debug", skip(self, ctx), err(level = "warn"))]
    pub(super) fn act(self, ctx: &ActCtx) -> CordialResult<()> {
        let socket = socket_path(ctx.store().root());
        match self {
            Self::Serve => host::serve(&socket),
            Self::Start => start(ctx, &socket),
            Self::Stop => stop(&socket),
            Self::Status => status(&socket),
        }
    }
}

/// Start a fresh session for this project, replacing one that is running.
#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
fn start(ctx: &ActCtx, socket: &std::path::Path) -> CordialResult<()> {
    if host::stop(socket)? {
        writeln!(host::stdout(), "Stopped the running session.")?;
    }
    let store_home = ctx
        .store_home()
        .clone()
        .unwrap_or_else(crate::default_store_home);
    let pid = host::start_session(ctx.project_root(), &store_home, ctx.store().root())?;
    writeln!(
        host::stdout(),
        "Session started (pid {pid}); it ends when this terminal does."
    )?;
    Ok(())
}

/// Ask the session to exit.
#[instrument(level = "debug", err(level = "warn"))]
fn stop(socket: &std::path::Path) -> CordialResult<()> {
    let answered = host::stop(socket)?;
    let message = if answered {
        "Session stopped."
    } else {
        "No session was running."
    };
    writeln!(host::stdout(), "{message}")?;
    Ok(())
}

/// Report whether a session answers.
#[instrument(level = "debug", err(level = "warn"))]
fn status(socket: &std::path::Path) -> CordialResult<()> {
    let running = std::os::unix::net::UnixStream::connect(socket).is_ok();
    writeln!(
        host::stdout(),
        "{}",
        if running {
            "A session is running."
        } else {
            "No session is running."
        }
    )?;
    Ok(())
}
