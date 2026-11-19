//! A run that a driver takes up again saves its state at a safe point between chunks and exits with
//! a status that says so, and the runs that take it up give what one run would have.

use cordial::{
    CHECKPOINT_AFTER_CHUNKS_VARIABLE, CHECKPOINT_AFTER_STAGES_VARIABLE, MEMORY_LIMIT_EXIT,
    MEMORY_RESUME_EXIT, MEMORY_STUCK_EXIT, MOST_RESUMES, Next, RESUMABLE_VARIABLE,
    RESUMES_VARIABLE, next_after, stalled_results,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

/// Enough functions for several chunks of the priming (a chunk is at least 64).
const FUNCTIONS: usize = 400;

fn write_crate(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='ckpt'\nversion='0.1.0'\nedition='2024'\n[features]\nextra=[]\nmore=[]\n[workspace]\n",
    )
    .into_diagnostic()?;
    let library: String = (0..FUNCTIONS)
        .map(|index| {
            format!("pub fn f{index}(x: Option<usize>) -> usize {{ x.unwrap() + {index} }}\n")
        })
        .collect();
    // What makes the run analyse more than the ordinary build: code that only exists under a
    // feature or under test, so that there are cfg contexts to analyse.
    let gated = "\n#[cfg(feature = \"extra\")]\npub fn extra_only(x: Option<usize>) -> usize { x.unwrap() }\n\
                 #[cfg(feature = \"more\")]\npub fn more_only(x: Option<usize>) -> usize { x.unwrap() + 1 }\n\
                 #[cfg(test)]\nmod tests {\n    #[test]\n    fn first() { assert_eq!(super::f0(Some(1)), 1); }\n}\n";
    std::fs::write(root.join("src/lib.rs"), format!("{library}{gated}")).into_diagnostic()?;
    // A file of integration tests, which the leaf passes read under the contexts their gates ask for.
    std::fs::create_dir_all(root.join("tests")).into_diagnostic()?;
    std::fs::write(
        root.join("tests/integration.rs"),
        "#[test]\nfn calls() { let _ = ckpt::f0(Some(1)); }\n\
         #[cfg(feature = \"extra\")]\n#[test]\nfn gated() { let _ = ckpt::f1(Some(1)); }\n",
    )
    .into_diagnostic()
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
fn the_driver_takes_the_run_up_again_only_when_it_says_it_is_to_be() {
    assert_eq!(next_after(Some(MEMORY_RESUME_EXIT), 0), Next::Resume);
    assert_eq!(
        next_after(Some(0), 3),
        Next::End(0),
        "a run that finished is over"
    );
    assert_eq!(
        next_after(Some(MEMORY_LIMIT_EXIT), 3),
        Next::End(MEMORY_LIMIT_EXIT),
        "and one stopped by its limit ends the run with that"
    );
    assert_eq!(
        next_after(Some(2), 0),
        Next::End(2),
        "any other status is the run's"
    );
    assert_eq!(
        next_after(None, 0),
        Next::End(1),
        "a child stopped by a signal ends it"
    );
    assert_eq!(
        next_after(Some(MEMORY_RESUME_EXIT), MOST_RESUMES),
        Next::End(1),
        "and a run is not taken up again for ever"
    );
}

#[test]
fn the_driver_runs_the_children_and_ends_with_what_one_run_gives() -> Result<()> {
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("ckpt");
    write_crate(&project)?;
    let (code, whole) = open(&project, &scratch.path().join("whole"), None)?;
    assert_eq!(code, Some(0), "an ordinary run finishes: {whole}");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(&project)
        .arg("--store-home")
        .arg(scratch.path().join("driven"))
        .arg("--memory-isolation")
        .arg("open")
        .env("CORDIAL_NO_PROGRESS", "1")
        .env("RUST_LOG", "warn,cordial::results=info")
        .env(CHECKPOINT_AFTER_CHUNKS_VARIABLE, "1")
        .output()
        .into_diagnostic()?;
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    let said = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code(), Some(0), "the run finishes: {said}");
    let restarts = said.matches("taking the run up again").count();
    assert!(
        restarts >= 2,
        "the driver started a new process several times: {restarts}\n{said}"
    );
    assert_eq!(printed, whole, "and the result is the one run's");

    // Each process found more of the work done than the one before it, and left less to do: what
    // they handed on was the results, in the store, and not a database.
    // The log colours its fields, as it does for a person's terminal: the colours are not the numbers.
    let plain = |line: &str| -> String {
        let mut kept = String::new();
        let mut characters = line.chars();
        while let Some(character) = characters.next() {
            if character == '\u{1b}' {
                for skipped in characters.by_ref() {
                    if skipped == 'm' {
                        break;
                    }
                }
            } else {
                kept.push(character);
            }
        }
        kept
    };
    let field = |line: &str, name: &str| -> Option<usize> {
        let line = plain(line);
        let after = line.split(&format!("{name}=")).nth(1)?;
        after.split_whitespace().next()?.parse().ok()
    };
    let rounds: Vec<(usize, usize)> = said
        .lines()
        .filter(|line| line.contains("whose evidence is in the store"))
        .filter_map(|line| Some((field(line, "done")?, field(line, "pending")?)))
        .collect();
    assert!(
        rounds.len() >= 3,
        "a process said what was left for each: {rounds:?}\n{said}"
    );
    assert_eq!(rounds[0].0, 0, "the first found nothing done: {rounds:?}");
    assert!(
        rounds
            .windows(2)
            .all(|pair| pair[1].0 > pair[0].0 && pair[1].1 < pair[0].1),
        "and each after it found more done and less to do: {rounds:?}"
    );
    assert_eq!(
        rounds[rounds.len() - 1].1,
        0,
        "until nothing was left: {rounds:?}"
    );
    let left_behind = std::fs::read_dir(
        scratch
            .path()
            .join("driven")
            .join("ckpt")
            .join("run-results"),
    )
    .map_or(0, |entries| entries.flatten().count());
    assert_eq!(left_behind, 0, "and the store went with the run");
    Ok(())
}

#[test]
fn a_process_to_take_a_run_up_that_has_no_state_to_use_stops_and_says_so() -> Result<()> {
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("ckpt");
    write_crate(&project)?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(&project)
        .arg("--store-home")
        .arg(scratch.path().join("empty"))
        .arg("open")
        .env("CORDIAL_NO_PROGRESS", "1")
        .env(RESUMABLE_VARIABLE, "1")
        .env(RESUMES_VARIABLE, "1")
        .output()
        .into_diagnostic()?;
    let said = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(
        output.status.code(),
        Some(MEMORY_STUCK_EXIT),
        "it stops: {said}"
    );
    assert!(
        said.contains("could not use the state saved for it"),
        "and says why: {said}"
    );
    Ok(())
}

#[test]
fn a_run_that_keeps_results_is_stuck_when_the_count_stops_growing() {
    assert!(!stalled_results(&[]));
    assert!(!stalled_results(&[64]));
    assert!(
        !stalled_results(&[64, 64]),
        "one that added nothing is not yet"
    );
    assert!(stalled_results(&[64, 64, 64]), "two in a row is");
    assert!(
        !stalled_results(&[64, 128, 192]),
        "each one more or many more is getting further"
    );
    assert!(
        !stalled_results(&[64, 65, 66]),
        "a single result each time is still more"
    );
    assert!(
        !stalled_results(&[64, 64, 100]),
        "and growth ends the run of stalls"
    );
}

/// Every line of `text` with the terminal's colours taken out.
fn plain_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| {
            let mut kept = String::new();
            let mut characters = line.chars();
            while let Some(character) = characters.next() {
                if character == '\u{1b}' {
                    for skipped in characters.by_ref() {
                        if skipped == 'm' {
                            break;
                        }
                    }
                } else {
                    kept.push(character);
                }
            }
            kept
        })
        .collect()
}

#[test]
fn a_run_whose_processes_end_after_every_stage_gives_what_one_run_gives() -> Result<()> {
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("ckpt");
    write_crate(&project)?;
    let (code, whole) = open(&project, &scratch.path().join("whole"), None)?;
    assert_eq!(code, Some(0), "an ordinary run finishes: {whole}");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(&project)
        .arg("--store-home")
        .arg(scratch.path().join("driven"))
        .arg("--memory-isolation")
        .arg("open")
        .env("CORDIAL_NO_PROGRESS", "1")
        .env(
            "RUST_LOG",
            "warn,cordial::results=info,cordial::semantic::contexts=info,cordial::semantic::leaves=info",
        )
        .env(CHECKPOINT_AFTER_STAGES_VARIABLE, "1")
        .output()
        .into_diagnostic()?;
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    let said = plain_lines(&String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.status.code(), Some(0), "the run finishes: {said:#?}");
    assert_eq!(printed, whole, "and gives the result one run gives");

    let kept: Vec<&String> = said
        .iter()
        .filter(|line| line.contains("a stage is done and kept in the store"))
        .collect();
    assert!(
        kept.iter().any(|line| line.contains("unit=\"ordinary\"")),
        "the ordinary part was kept: {kept:#?}"
    );
    assert!(
        kept.iter().any(|line| line.contains("unit=\"context-")),
        "and at least one cfg context: {kept:#?}"
    );
    for unit in ["contexts", "tracing-inventories", "leaves"] {
        assert!(
            kept.iter()
                .any(|line| line.contains(&format!("unit=\"{unit}\""))),
            "{unit} was kept: {kept:#?}"
        );
    }
    assert!(
        said.iter()
            .any(|line| line.contains("progress is kept in the store")
                && line.contains("unit=\"leaf-context-")),
        "and at least one leaf context: {said:#?}"
    );
    assert!(
        said.iter()
            .any(|line| line.contains("the leaf contexts are taken up from an earlier process")),
        "a process went on with the leaf contexts an earlier one read: {said:#?}"
    );
    let taken_up: Vec<&String> = said
        .iter()
        .filter(|line| line.contains("taken up from an earlier process"))
        .collect();
    assert!(
        taken_up.len() >= 2,
        "later processes took the stages up, and the contexts: {taken_up:#?}"
    );
    assert!(
        said.iter()
            .any(|line| line.contains("the cfg contexts are taken up from an earlier process")),
        "a process went on with the contexts an earlier one analysed: {said:#?}"
    );
    Ok(())
}
