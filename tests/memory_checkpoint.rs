//! A run that a driver takes up again saves its state at a safe point between chunks and exits with
//! a status that says so, and the runs that take it up give what one run would have.

use cordial::{CHECKPOINT_AFTER_CHUNKS_VARIABLE, MEMORY_RESUME_EXIT, RESUMABLE_VARIABLE};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

/// Enough functions for several chunks of the priming (a chunk is at least 64).
const FUNCTIONS: usize = 400;

fn write_crate(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='ckpt'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let library: String = (0..FUNCTIONS)
        .map(|index| {
            format!("pub fn f{index}(x: Option<usize>) -> usize {{ x.unwrap() + {index} }}\n")
        })
        .collect();
    std::fs::write(root.join("src/lib.rs"), library).into_diagnostic()
}

/// One process of `cordial open`: its exit code and what it printed.
fn open(project: &Path, store: &Path, resumable: Option<&str>) -> Result<(Option<i32>, String)> {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_cordial"));
    command
        .arg("--project")
        .arg(project)
        .arg("--store-home")
        .arg(store)
        .arg("open")
        .env("CORDIAL_NO_PROGRESS", "1");
    if let Some(after) = resumable {
        command
            .env(RESUMABLE_VARIABLE, "1")
            .env(CHECKPOINT_AFTER_CHUNKS_VARIABLE, after);
    }
    let output = command.output().into_diagnostic()?;
    Ok((
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    ))
}

#[test]
fn a_run_that_is_taken_up_again_saves_and_exits_and_the_runs_that_follow_give_the_same_result()
-> Result<()> {
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("ckpt");
    write_crate(&project)?;

    // What one run gives, with nothing to take up.
    let (code, whole) = open(&project, &scratch.path().join("whole"), None)?;
    assert_eq!(code, Some(0), "an ordinary run finishes: {whole}");

    // The same project, a process that saves and exits after every chunk that computed anything,
    // taken up again until one finishes.
    let store = scratch.path().join("resumed");
    let mut processes = 0;
    let finished = loop {
        processes += 1;
        assert!(processes <= 40, "it does not get further");
        let (code, printed) = open(&project, &store, Some("1"))?;
        match code {
            Some(MEMORY_RESUME_EXIT) => {}
            Some(0) => break printed,
            other => panic!("a process ended with {other:?}: {printed}"),
        }
    };
    assert!(
        processes >= 3,
        "it took up the run again several times, not once: {processes} processes"
    );
    assert_eq!(
        finished, whole,
        "and gave what the run that was not interrupted gave"
    );
    Ok(())
}
