//! The session server: runs the commands clients forward to it, in this
//! process, so the databases it keeps survive from one command to the next.

use crate::host::{
    Frame, FrameSink, PROTOCOL, Request, Stream, keep_between_runs, with_sink, write_frame,
};
use crate::run_cache::shapes_the_build;
use crate::{CordialError, CordialResult};
use clap::Parser;
use miette::IntoDiagnostic;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use tracing::instrument;

/// Sends a request's output down its connection as frames.
struct Connection {
    stream: Mutex<UnixStream>,
}

impl FrameSink for Connection {
    #[instrument(level = "trace", skip(self, bytes))]
    fn send(&self, stream: Stream, bytes: &[u8]) -> std::io::Result<()> {
        let mut socket = self.stream.lock().unwrap_or_else(PoisonError::into_inner);
        write_frame(&mut *socket, &Frame::Output(stream, bytes.to_vec()))
    }
}

/// Serve requests on `socket` until one asks the server to exit.
///
/// A socket left by a server that is gone is replaced; one that still answers
/// is an error, since two sessions would each hold half the state.
#[instrument(level = "debug", err(level = "warn"))]
pub fn serve(socket: &Path, owner: Option<u32>) -> CordialResult<()> {
    if UnixStream::connect(socket).is_ok() {
        return Err(CordialError::perf(
            socket.to_path_buf(),
            "a session is already serving here",
        ));
    }
    match std::fs::remove_file(socket) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let listener = UnixListener::bind(socket)?;
    // Only this user may connect, whichever directory the socket is in.
    std::fs::set_permissions(socket, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    keep_between_runs();
    if let Some(owner) = owner {
        watch_owner(socket.to_path_buf(), owner);
    }
    tracing::info!(socket = %socket.display(), "session serving");
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => match handle(stream) {
                Ok(true) => break,
                Ok(false) => {}
                // One bad connection must not end the session that holds the state.
                Err(error) => tracing::warn!(error = ?error, "could not answer a connection"),
            },
            Err(error) => tracing::warn!(error = ?error, "could not accept a connection"),
        }
    }
    drop(listener);
    std::fs::remove_file(socket)?;
    Ok(())
}

/// Exit when the terminal that started this session ends, removing the socket.
/// A server is somebody's working state, not a daemon to leave behind.
#[instrument(level = "debug")]
fn watch_owner(socket: std::path::PathBuf, owner: u32) {
    let spawned = std::thread::Builder::new()
        .name("cordial-session-owner".to_owned())
        .spawn(move || {
            while super::terminal::leader_alive(owner) {
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
            tracing::info!(
                owner,
                "the terminal that started this session ended; exiting"
            );
            let _ = std::fs::remove_file(&socket);
            std::process::exit(0);
        });
    if let Err(error) = spawned {
        tracing::warn!(error = ?error, "could not watch the terminal; this session ends only when stopped");
    }
}

/// The first build-shaping variable on which the client and this server differ.
/// The server's environment is fixed when it starts, so a command that would
/// build differently has to run in its own process.
#[instrument(level = "debug", skip(request))]
fn environment_conflict(request: &Request) -> Option<String> {
    let ours: std::collections::BTreeMap<String, String> = std::env::vars()
        .filter(|(name, _)| shapes_the_build(name) || name == "CORDIAL_HOME")
        .collect();
    ours.keys()
        .chain(request.env().keys())
        .find(|name| ours.get(*name) != request.env().get(*name))
        .cloned()
}

/// Answer one connection. Returns whether the server should now exit.
#[instrument(level = "debug", skip(stream), err(level = "warn"))]
#[hotpath::measure]
fn handle(stream: UnixStream) -> CordialResult<bool> {
    let mut line = String::new();
    BufReader::new(stream.try_clone()?).read_line(&mut line)?;
    if line.trim().is_empty() {
        // A probe: someone checked that the session answers and hung up.
        return Ok(false);
    }
    let request: Request = serde_json::from_str(&line)?;
    let mut reply = stream.try_clone()?;
    if request.version() != PROTOCOL {
        write_frame(
            &mut reply,
            &Frame::Refused(format!("protocol {} is not {PROTOCOL}", request.version())),
        )?;
        return Ok(false);
    }
    if request.shutdown() {
        write_frame(&mut reply, &Frame::Exit(0))?;
        return Ok(true);
    }
    if let Some(name) = environment_conflict(&request) {
        write_frame(
            &mut reply,
            &Frame::Refused(format!("{name} differs from the session's environment")),
        )?;
        return Ok(false);
    }
    let sink = Arc::new(Connection {
        stream: Mutex::new(stream),
    });
    let code = with_sink(sink.clone(), || run(request.argv()));
    write_frame(&mut reply, &Frame::Exit(code))?;
    Ok(false)
}

/// Run one command line as `main` would and return its exit code. Output goes
/// wherever the current thread's sink sends it.
#[instrument(level = "debug", skip(argv))]
#[hotpath::measure]
fn run(argv: &[String]) -> i32 {
    let cli = match crate::Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(error) => {
            let rendered = error.render().ansi().to_string();
            let stream = if error.use_stderr() {
                crate::host::stderr()
            } else {
                crate::host::stdout()
            };
            let mut stream = stream;
            let _ = stream.write_all(rendered.as_bytes());
            return error.exit_code();
        }
    };
    let outcome =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cli.act().into_diagnostic()));
    match outcome {
        Ok(Ok(())) => 0,
        Ok(Err(report)) => {
            let _ = writeln!(crate::host::stderr(), "{report:?}");
            1
        }
        Err(_) => {
            let _ = writeln!(
                crate::host::stderr(),
                "the command panicked; the session continues"
            );
            101
        }
    }
}
