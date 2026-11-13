//! The client side: hand a command to a running session, or decline so the
//! caller runs it itself.

use crate::host::{Frame, Request, STALE_BUILD, Stream, read_frame};
use crate::run_cache::shapes_the_build;
use clap::Parser;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use tracing::instrument;

/// The socket of the session for a project's store.
///
/// A Unix socket's path is limited to about a hundred bytes, which a store under
/// a deep directory exceeds, so it lives in the per-user runtime directory under
/// a name derived from the store: the same store always finds the same socket.
#[instrument(level = "trace")]
pub fn socket_path(store_root: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(store_root.as_os_str().as_encoded_bytes());
    let name: String = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_dir())
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("cordial-{name}.sock"))
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

/// Whether `command` begins a stretch of work, and so starts a session when none
/// is running.
#[instrument(level = "trace", skip(command))]
fn starts_a_session(command: &crate::Commands) -> bool {
    #[cfg(feature = "quality")]
    {
        matches!(command, crate::Commands::Open { .. })
    }
    #[cfg(not(feature = "quality"))]
    {
        let _ = command;
        false
    }
}

/// Launch `session serve` for the project, detached, and wait until it answers.
/// The server keeps the process session of whoever starts it, which is how it
/// knows when the terminal that wanted it has gone. Returns its process id.
#[instrument(level = "debug", err(level = "warn"))]
#[hotpath::measure]
pub fn start_session(
    project: &Path,
    store_home: &Path,
    store_root: &Path,
    owner: Option<u32>,
) -> std::io::Result<u32> {
    use std::os::unix::process::CommandExt;
    std::fs::create_dir_all(store_root)?;
    let socket = socket_path(store_root);
    let child = std::process::Command::new(std::env::current_exe()?)
        .arg("--project")
        .arg(project)
        .arg("--store-home")
        .arg(store_home)
        .args(["session", "serve"])
        .args(
            owner
                .iter()
                .flat_map(|owner| ["--owner".to_owned(), owner.to_string()]),
        )
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::fs::File::create(store_root.join("session.log"))?)
        .process_group(0)
        .spawn()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        if UnixStream::connect(&socket).is_ok() {
            return Ok(child.id());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err(std::io::Error::other(
        "the session did not start; see session.log in the store",
    ))
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
    if !forwardable(&cli.command) || std::env::var_os("CORDIAL_NO_SESSION").is_some() {
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
    let store_root = store_home.join(crate::project_slug_from_path(&project));
    let socket = socket_path(&store_root);
    let mut stream = match UnixStream::connect(&socket) {
        Ok(stream) => stream,
        // `open` is where work begins: with no session running it starts one, so
        // the commands after it find the database already loaded.
        // Without a terminal to end it with, a session would be left behind.
        Err(_) if starts_a_session(&cli.command) && super::terminal::terminal_owner().is_some() => {
            start_session(
                &project,
                &store_home,
                &store_root,
                super::terminal::terminal_owner(),
            )
            .ok()?;
            let _ = writeln!(
                std::io::stderr(),
                "Started a session; it ends when this terminal does (cordial session stop ends it sooner)."
            );
            UnixStream::connect(&socket).ok()?
        }
        Err(_) => return None,
    };
    let mut argv = argv;
    if cli.project.is_none() {
        argv.push("--project".to_owned());
        argv.push(project.display().to_string());
    }
    let request = serde_json::to_string(&Request::command(argv, build_environment())).ok()?;
    writeln!(stream, "{request}").ok()?;
    match relay(&mut stream) {
        Relayed::Finished(code) => code,
        // A session from before a rebuild must not answer with the old code: it
        // is replaced, and the command goes to the new one.
        Relayed::Stale => {
            tracing::info!("the session predates this build; replacing it");
            let _ = stop(&socket);
            if !(starts_a_session(&cli.command) && super::terminal::terminal_owner().is_some()) {
                return None;
            }
            start_session(
                &project,
                &store_home,
                &store_root,
                super::terminal::terminal_owner(),
            )
            .ok()?;
            let _ = writeln!(
                std::io::stderr(),
                "The session was from an older build of cordial; started a new one."
            );
            let mut stream = UnixStream::connect(&socket).ok()?;
            writeln!(stream, "{request}").ok()?;
            match relay(&mut stream) {
                Relayed::Finished(code) => code,
                Relayed::Stale => None,
            }
        }
    }
}

/// How a session answered a forwarded command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Relayed {
    /// The command is done, or the caller should run it itself (`None`).
    Finished(Option<i32>),
    /// The session was started from another build of cordial.
    Stale,
}

/// Copy the server's output to ours until its exit frame. A refusal, or a server
/// that says nothing before it goes, returns `None`; one that goes mid-command
/// has already printed, so that is an error exit rather than a second run.
#[instrument(level = "debug", skip(stream))]
#[hotpath::measure]
fn relay(stream: &mut UnixStream) -> Relayed {
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
            Ok(Some(Frame::Exit(code))) => return Relayed::Finished(Some(code)),
            Ok(Some(Frame::Refused(reason))) if reason == STALE_BUILD => return Relayed::Stale,
            Ok(Some(Frame::Refused(reason))) => {
                tracing::info!(reason, "the session declined; running here");
                return Relayed::Finished(None);
            }
            Ok(None) | Err(_) if !started => return Relayed::Finished(None),
            Ok(None) | Err(_) => {
                let _ = writeln!(
                    std::io::stderr(),
                    "the session ended before the command finished"
                );
                return Relayed::Finished(Some(1));
            }
        }
    }
}

/// Ask the session at `socket` to exit. Returns whether one answered.
#[instrument(level = "debug", err(level = "warn"))]
#[hotpath::measure]
pub fn stop(socket: &Path) -> std::io::Result<bool> {
    let Ok(mut stream) = UnixStream::connect(socket) else {
        return Ok(false);
    };
    writeln!(stream, "{}", serde_json::to_string(&Request::stop())?)?;
    Ok(matches!(read_frame(&mut stream)?, Some(Frame::Exit(0))))
}
