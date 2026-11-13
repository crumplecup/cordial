//! The client side: hand a command to a running session, or decline so the
//! caller runs it itself.

use crate::host::{Frame, Request, Stream, read_frame};
use crate::run_cache::shapes_the_build;
use clap::Parser;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use tracing::instrument;

/// The socket of the session for a project's store.
#[instrument(level = "trace")]
pub fn socket_path(store_root: &Path) -> PathBuf {
    store_root.join("session.sock")
}

/// Whether `command` can run in a session. These read and write only the store
/// and the project, and take no paths of their own, so the session's working
/// directory does not matter.
#[instrument(level = "trace", skip(command))]
fn forwardable(command: &crate::Commands) -> bool {
    #[cfg(feature = "quality")]
    {
        use crate::Commands;
        matches!(
            command,
            Commands::Open { .. }
                | Commands::Peek { .. }
                | Commands::Done { .. }
                | Commands::Skip { .. }
                | Commands::Submit { .. }
                | Commands::Next
                | Commands::Stats { .. }
        )
    }
    #[cfg(not(feature = "quality"))]
    {
        let _ = command;
        false
    }
}

/// The variables the server compares against its own environment.
#[instrument(level = "trace")]
pub fn build_environment() -> BTreeMap<String, String> {
    std::env::vars()
        .filter(|(name, _)| shapes_the_build(name) || name == "CORDIAL_HOME")
        .collect()
}

/// Run `args` in the project's session if one is serving and will take the
/// command. Returns the exit code, or `None` when the caller should run the
/// command itself: no session, a command that cannot be forwarded, or a
/// session that refused it.
#[instrument(level = "debug", skip(args))]
#[hotpath::measure]
pub fn forward(args: &[OsString]) -> Option<i32> {
    let argv: Vec<String> = args
        .iter()
        .map(|arg| arg.to_str().map(str::to_owned))
        .collect::<Option<_>>()?;
    let cli = crate::Cli::try_parse_from(&argv).ok()?;
    if !forwardable(&cli.command) {
        return None;
    }
    let project = match &cli.project {
        Some(path) => path.canonicalize().ok()?,
        None => std::env::current_dir().ok()?.canonicalize().ok()?,
    };
    let store_home = cli
        .store_home
        .clone()
        .unwrap_or_else(crate::default_store_home);
    let socket = socket_path(&store_home.join(crate::project_slug_from_path(&project)));
    let mut stream = UnixStream::connect(&socket).ok()?;
    let mut argv = argv;
    if cli.project.is_none() {
        argv.push("--project".to_owned());
        argv.push(project.display().to_string());
    }
    let request = serde_json::to_string(&Request::command(argv, build_environment())).ok()?;
    writeln!(stream, "{request}").ok()?;
    relay(&mut stream)
}

/// Copy the server's output to ours until its exit frame. A refusal, or a server
/// that says nothing before it goes, returns `None`; one that goes mid-command
/// has already printed, so that is an error exit rather than a second run.
#[instrument(level = "debug", skip(stream))]
fn relay(stream: &mut UnixStream) -> Option<i32> {
    let mut started = false;
    loop {
        match read_frame(stream) {
            Ok(Some(Frame::Output(which, bytes))) => {
                started = true;
                let _ = match which {
                    Stream::Stdout => std::io::stdout().write_all(&bytes),
                    Stream::Stderr => std::io::stderr().write_all(&bytes),
                };
            }
            Ok(Some(Frame::Exit(code))) => return Some(code),
            Ok(Some(Frame::Refused(reason))) => {
                tracing::info!(reason, "the session declined; running here");
                return None;
            }
            Ok(None) | Err(_) if !started => return None,
            Ok(None) | Err(_) => {
                let _ = writeln!(
                    std::io::stderr(),
                    "the session ended before the command finished"
                );
                return Some(1);
            }
        }
    }
}

/// Ask the session at `socket` to exit. Returns whether one answered.
#[instrument(level = "debug", err(level = "warn"))]
pub fn stop(socket: &Path) -> std::io::Result<bool> {
    let Ok(mut stream) = UnixStream::connect(socket) else {
        return Ok(false);
    };
    writeln!(stream, "{}", serde_json::to_string(&Request::stop())?)?;
    Ok(matches!(read_frame(&mut stream)?, Some(Frame::Exit(0))))
}
