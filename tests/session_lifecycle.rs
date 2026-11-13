//! The session's lifecycle through the real binary: `open` starts one, later
//! commands reach it, `session start` replaces it, and it ends with its terminal.

mod support {
    pub mod mini_workspace;
}
use miette::{IntoDiagnostic, Result};
use std::path::Path;
use std::process::{Command, Output, Stdio};
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
fn open_starts_a_session_and_the_commands_after_it_reach_it() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("mini");
    support::mini_workspace::write(&project)?;
    let home = scratch.path().join("store");
    let socket = home.join("mini/session.sock");

    let status = run(&project, &home, &["session", "status"])?;
    assert!(text(&status.stdout).contains("No session is running"));

    let opened = run(&project, &home, &["open"])?;
    assert!(opened.status.success(), "{}", text(&opened.stderr));
    assert!(
        text(&opened.stderr).contains("Started a session"),
        "{}",
        text(&opened.stderr)
    );
    assert!(socket.exists(), "open left a session serving");
    assert!(text(&run(&project, &home, &["session", "status"])?.stdout).contains("is running"));

    // Another command finds it and does not start a second.
    let stats = run(&project, &home, &["stats"])?;
    assert!(stats.status.success(), "{}", text(&stats.stderr));
    assert!(!text(&stats.stderr).contains("Started a session"));

    // Starting on purpose replaces it.
    let fresh = run(&project, &home, &["session", "start"])?;
    assert!(text(&fresh.stdout).contains("Stopped the running session"));
    assert!(text(&fresh.stdout).contains("Session started"));
    assert!(socket.exists());

    let stopped = run(&project, &home, &["session", "stop"])?;
    assert!(text(&stopped.stdout).contains("Session stopped"));
    wait_until("the socket to go", || !socket.exists());
    Ok(())
}

#[test]
fn a_session_ends_when_the_terminal_that_started_it_does() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("mini");
    support::mini_workspace::write(&project)?;
    let home = scratch.path().join("store");
    let socket = home.join("mini/session.sock");

    // `setsid` makes this child the leader of a session of its own, as a terminal's
    // shell is, and starts the session from inside it.
    let start = format!(
        "{} --project {} --store-home {} session start; sleep 600",
        env!("CARGO_BIN_EXE_cordial"),
        project.display(),
        home.display()
    );
    let mut terminal = Command::new("setsid")
        .args(["sh", "-c", &start])
        .env_remove("CORDIAL_NO_SESSION")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .into_diagnostic()?;
    wait_until("the session to start", || socket.exists());

    // The terminal closes: its shell goes, and the session follows.
    terminal.kill().into_diagnostic()?;
    terminal.wait().into_diagnostic()?;
    wait_until("the session to end with its terminal", || !socket.exists());
    Ok(())
}
