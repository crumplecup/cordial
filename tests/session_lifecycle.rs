//! The session's lifecycle through the real binary: `open` starts one, later
//! commands reach it, `session start` replaces it, and it ends with its terminal.

mod support {
    pub mod mini_workspace;
}
use miette::{IntoDiagnostic, Result};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

fn cordial(project: &Path, home: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cordial"));
    command
        .arg("--project")
        .arg(project)
        .arg("--store-home")
        .arg(home)
        .args(args)
        .env("CORDIAL_NO_PROGRESS", "1")
        .env_remove("CORDIAL_NO_SESSION")
        .stdin(Stdio::null());
    command
}

fn run(project: &Path, home: &Path, args: &[&str]) -> Result<Output> {
    cordial(project, home, args).output().into_diagnostic()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn a_session_serves_forwarded_commands_and_start_replaces_it() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("mini");
    support::mini_workspace::write(&project)?;
    let home = scratch.path().join("store");
    let socket = home.join("mini/session.sock");
    assert!(text(&run(&project, &home, &["session", "status"])?.stdout).contains("No session"));

    let started = run(&project, &home, &["session", "start"])?;
    assert!(started.status.success(), "{}", text(&started.stderr));
    assert!(socket.exists());
    assert!(text(&run(&project, &home, &["session", "status"])?.stdout).contains("is running"));

    // The commands after it reach it: `open` is answered by the session.
    let opened = run(&project, &home, &["open"])?;
    assert!(opened.status.success(), "{}", text(&opened.stderr));
    let stats = run(&project, &home, &["stats"])?;
    assert!(stats.status.success(), "{}", text(&stats.stderr));

    let fresh = run(&project, &home, &["session", "start"])?;
    assert!(text(&fresh.stdout).contains("Stopped the running session"));
    assert!(socket.exists());

    let stopped = run(&project, &home, &["session", "stop"])?;
    assert!(text(&stopped.stdout).contains("Session stopped"));
    wait_until("the socket to go", || !socket.exists());
    Ok(())
}

/// A session leader that does nothing, standing in for a terminal's shell.
fn terminal() -> Result<Child> {
    Command::new("setsid")
        .args(["sleep", "600"])
        .stdin(Stdio::null())
        .spawn()
        .into_diagnostic()
}

#[test]
fn a_session_ends_when_its_owner_does() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("mini");
    support::mini_workspace::write(&project)?;
    let home = scratch.path().join("store");
    let socket = home.join("mini/session.sock");

    let mut owner = terminal()?;
    let mut server = cordial(
        &project,
        &home,
        &["session", "serve", "--owner", &owner.id().to_string()],
    )
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .into_diagnostic()?;
    wait_until("the session to start", || socket.exists());
    std::thread::sleep(Duration::from_secs(3));
    assert!(socket.exists(), "it keeps serving while its owner runs");

    owner.kill().into_diagnostic()?;
    owner.wait().into_diagnostic()?;
    wait_until("the session to end with its owner", || !socket.exists());
    server.wait().into_diagnostic()?;
    Ok(())
}
