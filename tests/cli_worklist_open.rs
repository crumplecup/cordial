//! Opening a worklist: the compiled instructions, building once, config changes, and peek/next.

use miette::IntoDiagnostic;
use std::collections::BTreeSet;
use std::fs;

#[path = "support/cli_worklist_project.rs"]
mod cli_worklist_project;

#[path = "support/cli_worklist_reading.rs"]
mod cli_worklist_reading;

use cli_worklist_project::{fixture, run};
use cli_worklist_reading::{list, stdout};

#[test]
fn compiled_instructions_name_real_commands() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let output = run(project.path(), home.path(), &["instructions"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout(&output), cordial::WORKLIST_INSTRUCTIONS);
    assert!(!store.worklist_path().exists());

    let commands: BTreeSet<&str> = cordial::WORKLIST_INSTRUCTIONS
        .split('`')
        .skip(1)
        .step_by(2)
        .filter_map(|quoted| quoted.strip_prefix("cordial "))
        .filter_map(|invocation| invocation.split_whitespace().next())
        .collect();
    assert!(commands.len() >= 6);
    for command in commands {
        let help = run(project.path(), home.path(), &[command, "--help"])?;
        assert!(
            help.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&help.stderr)
        );
    }
    Ok(())
}

#[test]
fn open_builds_once_and_stats_reports_hard_invalidation() -> miette::Result<()> {
    cordial::init_tracing();
    let project = tempfile::tempdir().into_diagnostic()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(project.path().join("src")).into_diagnostic()?;
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"worklist-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        project.path().join("src/lib.rs"),
        include_str!("fixtures/panics/cli_quality.rs"),
    )
    .into_diagnostic()?;

    let before = run(project.path(), home.path(), &["stats"])?;
    assert!(before.status.success());
    assert!(stdout(&before).contains("No list yet."));
    assert!(stdout(&before).contains("NEXT: cordial open"));

    let first = run(project.path(), home.path(), &["open"])?;
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(stdout(&first).contains("Item 1 of"), "{}", stdout(&first));
    let slug = cordial::project_slug_from_path(project.path());
    let store = cordial::StoreLayout::from_root(home.path().join(&slug), slug);
    let list = cordial::Worklist::read(&store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("open did not write a list"))?;
    assert_eq!(*list.record().generation(), 1);
    assert!(list.current().is_some());

    let again = run(project.path(), home.path(), &["open"])?;
    assert!(again.status.success());
    let current = list
        .current()
        .ok_or_else(|| miette::miette!("open did not activate a current item"))?;
    assert!(stdout(&again).contains(current.id().as_str()));
    assert!(!stdout(&again).contains("Building one"));
    assert_eq!(
        *cordial::Worklist::read(&store)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("list missing after second open"))?
            .record()
            .generation(),
        1
    );

    let stats = run(project.path(), home.path(), &["stats", "--verbose"])?;
    assert!(stats.status.success());
    assert!(stdout(&stats).contains("[>]"), "{}", stdout(&stats));

    fs::write(
        project.path().join("cordial.toml"),
        "[panics]\nenabled = false\n",
    )
    .into_diagnostic()?;
    let stale = run(project.path(), home.path(), &["stats"])?;
    assert!(stale.status.success());
    assert!(stdout(&stale).contains("configuration changed"));
    assert!(stdout(&stale).contains("NEXT: cordial open"));
    let rebuilt = run(project.path(), home.path(), &["open"])?;
    assert!(
        rebuilt.status.success(),
        "{}",
        String::from_utf8_lossy(&rebuilt.stderr)
    );
    assert!(stdout(&rebuilt).contains("Rebuilding; 0 checked-off item(s) will be discarded."));
    assert_eq!(
        *cordial::Worklist::read(&store)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("list missing after rebuild"))?
            .record()
            .generation(),
        2
    );
    Ok(())
}

#[test]
fn open_then_submit_accepts_unchanged_config_with_maps() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    fs::write(
        project.path().join("cordial.toml"),
        "[cfg_hygiene]\ncrate_verifier = { amenable_kani = 'kani', amenable_creusot = 'creusot', amenable_verus = 'verus' }\n",
    )
    .into_diagnostic()?;

    let opened = run(project.path(), home.path(), &["open", "--json"])?;
    assert!(
        opened.status.success(),
        "{}",
        String::from_utf8_lossy(&opened.stderr)
    );
    assert_eq!(*list(&store)?.record().generation(), 1);

    let submitted = run(
        project.path(),
        home.path(),
        &["submit", "--tier", "quick", "--json"],
    )?;
    assert!(
        submitted.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&submitted),
        String::from_utf8_lossy(&submitted.stderr)
    );
    assert_eq!(*list(&store)?.record().generation(), 2);
    Ok(())
}

#[test]
fn peek_activates_a_fresh_list_and_next_does_not_change_it() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let before = run(project.path(), home.path(), &["peek"])?;
    assert!(before.status.success());
    assert!(stdout(&before).contains("No list yet"));
    assert!(cordial::Worklist::read(&store).into_diagnostic()?.is_none());

    let gate = run(project.path(), home.path(), &["quality"])?;
    assert!(
        gate.status.success(),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let peek = run(project.path(), home.path(), &["peek"])?;
    assert!(peek.status.success());
    assert!(stdout(&peek).contains("Item 1 of"));
    let activated = list(&store)?;
    assert!(activated.current().is_some());

    let next = run(project.path(), home.path(), &["next"])?;
    assert!(!next.status.success());
    assert!(String::from_utf8_lossy(&next.stderr).contains("cordial done"));
    assert_eq!(list(&store)?, activated);
    Ok(())
}
