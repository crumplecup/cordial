//! The session server runs forwarded commands and answers like a local run, and
//! the databases it keeps follow the disk.

mod support {
    pub mod mini_workspace;
}
use clap::Parser;
use cordial::host::{
    Frame, FrameSink, Request, STALE_BUILD, Stream, build_environment, keep, key_of, read_frame,
    serve, socket_path, stop, take_current, with_sink,
};
use cordial::{Cli, SemanticCargoOptions, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};

/// Collects what a local run writes, to compare with what the session sends.
#[derive(Default)]
struct Collect(Mutex<Vec<(bool, Vec<u8>)>>);

impl FrameSink for Collect {
    fn send(&self, stream: Stream, bytes: &[u8]) -> std::io::Result<()> {
        self.0
            .lock()
            .map_err(|_| std::io::Error::other("poisoned"))?
            .push((stream == Stream::Stdout, bytes.to_vec()));
        Ok(())
    }
}

fn joined(parts: &[(bool, Vec<u8>)], stdout: bool) -> Vec<u8> {
    parts
        .iter()
        .filter(|(is_out, _)| *is_out == stdout)
        .flat_map(|(_, bytes)| bytes.clone())
        .collect()
}

/// Send `argv` to the server at `socket` and collect the frames it answers with.
fn ask(socket: &std::path::Path, request: &Request) -> Result<Vec<Frame>> {
    let mut stream = UnixStream::connect(socket).into_diagnostic()?;
    writeln!(
        stream,
        "{}",
        serde_json::to_string(request).into_diagnostic()?
    )
    .into_diagnostic()?;
    let mut frames = Vec::new();
    while let Some(frame) = read_frame(&mut stream).into_diagnostic()? {
        let done = matches!(frame, Frame::Exit(_) | Frame::Refused(_));
        frames.push(frame);
        if done {
            break;
        }
    }
    Ok(frames)
}

fn output_of(frames: &[Frame], stdout: bool) -> Vec<u8> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            Frame::Output(stream, bytes) if (*stream == Stream::Stdout) == stdout => {
                Some(bytes.clone())
            }
            _ => None,
        })
        .flatten()
        .collect()
}

#[test]
fn a_server_answers_like_a_local_run_and_stops_when_asked() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let socket = socket_path(scratch.path());
    let server = {
        let socket = socket.clone();
        std::thread::spawn(move || serve(&socket, None))
    };
    let mut waited = 0;
    while UnixStream::connect(&socket).is_err() {
        std::thread::sleep(std::time::Duration::from_millis(20));
        waited += 1;
        assert!(waited < 500, "the server never came up");
    }

    // The same command, run locally with its output collected.
    let tiers = vec!["cordial".to_owned(), "tiers".to_owned()];
    let local = Arc::new(Collect::default());
    with_sink(local.clone(), || {
        Cli::try_parse_from(&tiers)
            .into_diagnostic()?
            .act()
            .into_diagnostic()
    })?;
    let local = local
        .0
        .lock()
        .map_err(|_| miette::miette!("poisoned"))?
        .clone();
    let remote = ask(&socket, &Request::command(tiers, build_environment()))?;
    assert_eq!(remote.last(), Some(&Frame::Exit(0)));
    assert_eq!(output_of(&remote, true), joined(&local, true));
    assert!(!output_of(&remote, true).is_empty());

    // A usage error is the client's exit code and message, not the server's.
    let bad = ask(
        &socket,
        &Request::command(
            vec!["cordial".into(), "no-such-command".into()],
            build_environment(),
        ),
    )?;
    assert_eq!(bad.last(), Some(&Frame::Exit(2)));
    assert!(String::from_utf8_lossy(&output_of(&bad, false)).contains("no-such-command"));

    // A build-shaping variable the server does not share sends the command home.
    let mut env = build_environment();
    env.insert("RUSTFLAGS".to_owned(), "--cfg elsewhere".to_owned());
    let refused = ask(
        &socket,
        &Request::command(vec!["cordial".into(), "tiers".into()], env),
    )?;
    assert!(matches!(refused.last(), Some(Frame::Refused(reason)) if reason.contains("RUSTFLAGS")));

    // A client from another build is told so, and the command is not run: a
    // session from before a rebuild must not answer with the old code.
    let mut other = serde_json::to_value(Request::command(
        vec!["cordial".into(), "tiers".into()],
        build_environment(),
    ))
    .into_diagnostic()?;
    other["build"] = "another build".into();
    let mut stream = UnixStream::connect(&socket).into_diagnostic()?;
    writeln!(stream, "{other}").into_diagnostic()?;
    assert_eq!(
        read_frame(&mut stream).into_diagnostic()?,
        Some(Frame::Refused(STALE_BUILD.to_owned()))
    );

    // A second server on a live socket is an error; the first one is untouched.
    assert!(serve(&socket, None).is_err());
    assert!(stop(&socket).into_diagnostic()?);
    server
        .join()
        .map_err(|_| miette::miette!("server panicked"))?
        .into_diagnostic()?;
    assert!(!socket.exists(), "the socket is removed on exit");
    assert!(!stop(&socket).into_diagnostic()?, "nothing is serving now");
    Ok(())
}

#[test]
fn a_kept_database_follows_edits_and_is_dropped_when_the_graph_changes() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let root = scratch.path();
    support::mini_workspace::write(root)?;
    let key = key_of(root, &SemanticCargoOptions::default());
    assert!(take_current(&key).is_none(), "nothing kept yet");

    keep(key.clone(), SourceDatabase::load(root).into_diagnostic()?);
    assert!(take_current(&key).is_some(), "an unchanged tree reuses it");
    assert!(take_current(&key).is_none(), "taking it removes it");

    keep(key.clone(), SourceDatabase::load(root).into_diagnostic()?);
    let source = root.join("src/lib.rs");
    let text = std::fs::read_to_string(&source).into_diagnostic()?;
    std::fs::write(&source, format!("// edited\n{text}")).into_diagnostic()?;
    let mut database =
        take_current(&key).ok_or_else(|| miette::miette!("dropped on a source edit"))?;
    assert_eq!(
        database.sync_from_disk().into_diagnostic()?,
        cordial::SyncOutcome::Current
    );

    keep(key.clone(), database);
    let manifest = root.join("Cargo.toml");
    let original = std::fs::read_to_string(&manifest).into_diagnostic()?;
    std::fs::write(&manifest, format!("{original}\n# edited\n")).into_diagnostic()?;
    assert!(
        take_current(&key).is_none(),
        "a changed manifest needs a reload"
    );
    Ok(())
}
