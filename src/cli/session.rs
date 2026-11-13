//! `cordial session`: keep one process holding the loaded workspace.

use clap::Subcommand;
use std::io::Write;
use std::time::{Duration, Instant};
use tracing::instrument;

use crate::host::{self, socket_path};
use crate::{CordialError, CordialResult};

use super::commands::ActCtx;

/// Subcommands of `cordial session`.
#[derive(Subcommand)]
pub enum SessionCommands {
    /// Start a session for this project in the background.
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

/// Launch `session serve` detached and wait until it answers.
#[instrument(level = "debug", skip(ctx), err(level = "warn"))]
fn start(ctx: &ActCtx, socket: &std::path::Path) -> CordialResult<()> {
    use std::os::unix::process::CommandExt;
    if std::os::unix::net::UnixStream::connect(socket).is_ok() {
        writeln!(host::stdout(), "A session is already running.")?;
        return Ok(());
    }
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .arg("--project")
        .arg(ctx.project_root())
        .arg("--store-home")
        .arg(
            ctx.store_home()
                .clone()
                .unwrap_or_else(crate::default_store_home),
        )
        .args(["session", "serve"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::fs::File::create(
            ctx.store().root().join("session.log"),
        )?)
        .process_group(0);
    let child = command.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if std::os::unix::net::UnixStream::connect(socket).is_ok() {
            writeln!(host::stdout(), "Session started (pid {}).", child.id())?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(CordialError::perf(
        socket.to_path_buf(),
        "the session did not start; see session.log in the store",
    ))
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
