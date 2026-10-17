use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use miette::{IntoDiagnostic, WrapErr};

fn run(root: &Path, home: &Path, args: &[&str]) -> miette::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(root)
        .arg("--store-home")
        .arg(home)
        .args(args)
        .env("CORDIAL_NO_PROGRESS", "1")
        .output()
        .into_diagnostic()
        .wrap_err_with(|| format!("cordial {}", args.join(" ")))
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn fixture() -> miette::Result<(tempfile::TempDir, tempfile::TempDir, cordial::StoreLayout)> {
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
    let slug = cordial::project_slug_from_path(project.path());
    let store = cordial::StoreLayout::from_root(home.path().join(&slug), slug);
    Ok((project, home, store))
}

fn list(store: &cordial::StoreLayout) -> miette::Result<cordial::Worklist> {
    cordial::Worklist::read(store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("worklist missing"))
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
fn claims_are_atomic_idempotent_and_advance_only_when_current_is_acted_on() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let gate = run(project.path(), home.path(), &["quality"])?;
    assert!(
        gate.status.success(),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let initial = list(&store)?;
    assert!(initial.cursor().is_none());
    assert!(initial.items().len() >= 2);
    let first = initial.items()[0].id().as_str().to_string();
    let second = initial.items()[1].id().as_str().to_string();

    let bare = run(project.path(), home.path(), &["done"])?;
    assert!(!bare.status.success());
    assert!(stdout(&bare).contains("NEXT: cordial open"));
    assert_eq!(list(&store)?, initial);

    let bad = run(project.path(), home.path(), &["done", &first, "unknown-id"])?;
    assert!(!bad.status.success());
    assert_eq!(list(&store)?, initial);

    let named = run(project.path(), home.path(), &["done", &second])?;
    assert!(
        named.status.success(),
        "{}",
        String::from_utf8_lossy(&named.stderr)
    );
    assert!(stdout(&named).contains(&format!("Checked off {second}:")));
    assert_eq!(
        list(&store)?.current().map(|item| item.id().as_str()),
        Some(first.as_str())
    );

    let repeated = run(project.path(), home.path(), &["done", &second])?;
    assert!(repeated.status.success());
    assert!(stdout(&repeated).contains("was already checked off"));
    assert_eq!(
        list(&store)?.current().map(|item| item.id().as_str()),
        Some(first.as_str())
    );

    let cannot_skip_done = run(project.path(), home.path(), &["skip", &second])?;
    assert!(!cannot_skip_done.status.success());
    assert_eq!(*list(&store)?.items()[1].state(), cordial::ItemState::Done);

    let skip = run(project.path(), home.path(), &["skip"])?;
    assert!(skip.status.success());
    assert!(stdout(&skip).contains(&format!("Set aside {first}:")));
    assert_ne!(
        list(&store)?.current().map(|item| item.id().as_str()),
        Some(first.as_str())
    );

    let recover = run(project.path(), home.path(), &["done", &first])?;
    assert!(recover.status.success());
    assert_eq!(*list(&store)?.items()[0].state(), cordial::ItemState::Done);
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

#[test]
fn skipping_the_last_open_item_completes_the_list_without_checking_it_off() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let open = run(project.path(), home.path(), &["open"])?;
    assert!(
        open.status.success(),
        "{}",
        String::from_utf8_lossy(&open.stderr)
    );
    let ids: Vec<String> = list(&store)?
        .items()
        .iter()
        .map(|item| item.id().as_str().to_string())
        .collect();
    let mut args = vec!["skip"];
    args.extend(ids.iter().map(String::as_str));
    let skipped = run(project.path(), home.path(), &args)?;
    assert!(
        skipped.status.success(),
        "{}",
        String::from_utf8_lossy(&skipped.stderr)
    );
    assert!(stdout(&skipped).contains("The list is complete."));
    assert!(stdout(&skipped).contains("item(s) were skipped"));
    assert!(list(&store)?.current().is_none());
    assert_eq!(list(&store)?.open_count(), ids.len());

    let reopened = run(project.path(), home.path(), &["open"])?;
    assert!(reopened.status.success());
    assert!(stdout(&reopened).contains("NEXT: run `cordial submit`"));
    let completed = run(project.path(), home.path(), &["done", &ids[0]])?;
    assert!(completed.status.success());
    assert_eq!(*list(&store)?.items()[0].state(), cordial::ItemState::Done);
    Ok(())
}
