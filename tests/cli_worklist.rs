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
