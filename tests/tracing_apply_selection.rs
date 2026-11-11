//! Which findings apply acts on: the checklist cannot bypass policy, and the current IR beats a stale list.

use cordial::run_tracing_instrument_apply;
use miette::{IntoDiagnostic, WrapErr};
use std::fs;
#[cfg(feature = "cli")]
use std::process::Command;

#[path = "support/tracing_apply_checklist.rs"]
mod tracing_apply_checklist;

#[path = "support/tracing_apply_crate.rs"]
mod tracing_apply_crate;

use tracing_apply_crate::{checklist_for, write_apply_fixture};

#[test]
fn old_checklist_cannot_bypass_shared_file_or_rule_policy() -> miette::Result<()> {
    cordial::init_tracing();
    let original = "pub fn pending() {}\n";
    for policy in [
        "[lints.tracing]\nenabled = false\n",
        "[lints.tracing.rules]\nTRACING-MISSING-INSTRUMENT = false\n",
        "[lints.tracing]\nskip_paths = { TRACING-MISSING-INSTRUMENT = [\"src/lib.rs\"] }\n",
        "[[lints.tracing.paths]]\npath = \"fixture_crate/src/lib.rs\"\nbase = \"workspace\"\nenabled = false\n",
    ] {
        let fixture = write_apply_fixture(original, &checklist_for(&[("pending", 1)]))?;
        fs::write(fixture.workspace.join("cordial.toml"), policy).into_diagnostic()?;
        let result =
            run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
                .into_diagnostic()?;
        assert_eq!(result.changed_functions(), 0, "{policy}");
        assert_eq!(
            fs::read_to_string(&fixture.src).into_diagnostic()?,
            original
        );
    }
    Ok(())
}

#[test]
fn existing_annotation_repairs_preserve_disabled_rule_arguments() -> miette::Result<()> {
    cordial::init_tracing();
    let original = "#[tracing::instrument(level = \"info\")]\npub fn read_data(path: &std::path::Path) -> Result<(), std::io::Error> { std::fs::read(path)?; Ok(()) }\n";
    let fixture = write_apply_fixture(original, &checklist_for(&[("read_data", 2)]))?;
    fs::write(
        fixture.workspace.join("cordial.toml"),
        r#"
[lints.tracing.rules]
TRACING-LEVEL-MISMATCH = false
TRACING-ERR-MISSING = false
TRACING-ERROR-PATH-SILENT = false
"#,
    )
    .into_diagnostic()?;
    let result = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()?;
    assert_eq!(result.changed_functions(), 1);
    let updated = fs::read_to_string(&fixture.src).into_diagnostic()?;
    assert!(updated.contains("level = \"info\""), "{updated}");
    assert!(updated.contains("skip(path)"), "{updated}");
    assert!(!updated.contains("err("), "{updated}");
    Ok(())
}

#[cfg(feature = "cli")]
#[test]
fn default_apply_preserves_compliant_annotations_and_is_idempotent() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"#[tracing::instrument(level = "trace", skip_all, fields(marker = "keep me"), ret)]
pub fn stable(path: &str) -> usize { path.len() }

#[tracing::instrument(level = "trace", skip_all, err(level = "warn"))]
pub fn fallible() -> Result<(), std::io::Error> { Ok(()) }

pub mod pending;
"#;
    let fixture = write_apply_fixture(source, "")?;
    let pending = fixture.src.with_file_name("pending.rs");
    fs::write(&pending, "pub fn run(data: &str) -> usize { data.len() }\n").into_diagnostic()?;
    let store_home = tempfile::tempdir().into_diagnostic()?;
    let run = |args: &[&str]| -> miette::Result<std::process::Output> {
        let output = Command::new(env!("CARGO_BIN_EXE_cordial"))
            .arg("--project")
            .arg(&fixture.workspace)
            .arg("--store-home")
            .arg(store_home.path())
            .args(args)
            .output()
            .into_diagnostic()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output)
    };
    run(&["quality", "--etiquette", "tracing"])?;
    let applied = run(&["quality", "--apply", "--etiquette", "tracing"])?;
    assert_eq!(
        fs::read_to_string(&fixture.src).into_diagnostic()?,
        source,
        "apply must preserve compliant source byte for byte"
    );
    let repaired = fs::read_to_string(&pending).into_diagnostic()?;
    assert!(repaired.contains("instrument("));
    #[cfg(feature = "quality")]
    assert!(String::from_utf8_lossy(&applied.stdout).contains("1 worklist item(s) checked off"));
    #[cfg(not(feature = "quality"))]
    let _ = applied;
    run(&["quality", "--apply", "--etiquette", "tracing"])?;
    assert_eq!(fs::read_to_string(&fixture.src).into_diagnostic()?, source);
    assert_eq!(fs::read_to_string(&pending).into_diagnostic()?, repaired);
    run(&["quality", "--etiquette", "tracing", "--deny-open"])?;
    Ok(())
}

#[cfg(feature = "cli")]
#[test]
fn default_apply_selects_current_ir_instead_of_stale_checklist() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture("pub fn run() {}\n", "")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    let store = store_home
        .path()
        .join(cordial::project_slug_from_path(&fixture.workspace))
        .join("findings");
    fs::create_dir_all(&store)
        .into_diagnostic()
        .wrap_err("findings dir")?;
    fs::write(
        store.join("tracing-instrument.checklist.md"),
        checklist_for(&[("fixture_crate::stale", 1)]),
    )
    .into_diagnostic()
    .wrap_err("stale checklist")?;

    let output = Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(&fixture.workspace)
        .arg("--store-home")
        .arg(store_home.path())
        .args(["quality", "--apply", "--etiquette", "tracing"])
        .output()
        .into_diagnostic()
        .wrap_err("default tracing apply")?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    #[cfg(feature = "quality")]
    assert!(String::from_utf8_lossy(&output.stdout).contains("no current worklist"));
    let applied = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("applied source")?;
    assert!(applied.contains("#[tracing::instrument") || applied.contains("#[instrument"));
    Ok(())
}

#[cfg(feature = "cli")]
#[test]
fn default_apply_respects_tracing_exception() -> miette::Result<()> {
    cordial::init_tracing();
    let source = "pub fn run() {}\n";
    let fixture = write_apply_fixture(source, "")?;
    let store_home = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store home")?;
    let exception_dir = store_home
        .path()
        .join(cordial::project_slug_from_path(&fixture.workspace))
        .join("exceptions/tracing");
    fs::create_dir_all(&exception_dir)
        .into_diagnostic()
        .wrap_err("exception dir")?;
    fs::write(
        exception_dir.join("fixture_crate.json"),
        r#"[{"file":"src/lib.rs","rule_id":"TRACING-MISSING-INSTRUMENT","reason":"fixture exception"}]"#,
    )
    .into_diagnostic()
    .wrap_err("tracing exception")?;

    let output = Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(&fixture.workspace)
        .arg("--store-home")
        .arg(store_home.path())
        .args(["quality", "--apply", "--etiquette", "tracing"])
        .output()
        .into_diagnostic()
        .wrap_err("default tracing apply")?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let applied = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("applied source")?;
    assert_eq!(applied, source);
    Ok(())
}

#[cfg(all(feature = "cli", feature = "quality"))]
#[test]
fn apply_checks_off_written_tracing_findings_but_not_dry_runs() -> miette::Result<()> {
    cordial::init_tracing();
    let source = "pub fn run() { println!(\"hello\"); }\n";
    let fixture = write_apply_fixture(source, "")?;
    let store_home = tempfile::tempdir().into_diagnostic()?;
    let slug = cordial::project_slug_from_path(&fixture.workspace);
    let store = cordial::StoreLayout::from_root(store_home.path().join(&slug), slug);
    let run = |args: &[&str]| -> miette::Result<std::process::Output> {
        Command::new(env!("CARGO_BIN_EXE_cordial"))
            .arg("--project")
            .arg(&fixture.workspace)
            .arg("--store-home")
            .arg(store_home.path())
            .args(args)
            .env("CORDIAL_NO_PROGRESS", "1")
            .output()
            .into_diagnostic()
    };

    let gate = run(&["quality", "--etiquette", "tracing"])?;
    assert!(
        gate.status.success(),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let explain = run(&["explain", "tracing"])?;
    assert!(explain.status.success());
    assert!(
        String::from_utf8_lossy(&explain.stdout)
            .contains("cordial quality --apply --etiquette tracing")
    );
    let before = cordial::Worklist::read(&store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing worklist"))?;
    let instrument_ids: Vec<_> = before
        .items()
        .iter()
        .filter(|item| item.checklist() == "tracing-instrument.checklist.md")
        .map(|item| item.id().as_str().to_string())
        .collect();
    assert!(!instrument_ids.is_empty());
    let manual_ids: Vec<_> = before
        .items()
        .iter()
        .filter(|item| item.checklist() == "tracing-print.checklist.md")
        .map(|item| item.id().as_str().to_string())
        .collect();
    assert!(!manual_ids.is_empty());

    let dry = run(&["quality", "--apply", "--etiquette", "tracing", "--dry-run"])?;
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    assert!(String::from_utf8_lossy(&dry.stdout).contains("0 worklist item(s) checked off"));
    assert!(
        String::from_utf8_lossy(&dry.stdout).contains(&format!(
            "Dry run: {} worklist item(s) would be checked off; {} would remain open for manual repair",
            instrument_ids.len(),
            manual_ids.len()
        )),
        "{}",
        String::from_utf8_lossy(&dry.stdout)
    );
    assert_eq!(fs::read_to_string(&fixture.src).into_diagnostic()?, source);
    assert_eq!(
        cordial::Worklist::read(&store).into_diagnostic()?,
        Some(before.clone())
    );

    let applied = run(&["quality", "--apply", "--etiquette", "tracing"])?;
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let after = cordial::Worklist::read(&store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing updated worklist"))?;
    for id in &instrument_ids {
        let item = after.items().iter().find(|item| item.id().as_str() == id);
        assert_eq!(
            item.map(|item| item.state()),
            Some(&cordial::ItemState::Done)
        );
    }
    for id in &manual_ids {
        let item = after.items().iter().find(|item| item.id().as_str() == id);
        assert_eq!(
            item.map(|item| item.state()),
            Some(&cordial::ItemState::Open)
        );
    }
    let remaining = after
        .items()
        .iter()
        .filter(|item| item.checklist().starts_with("tracing-"))
        .filter(|item| *item.state() == cordial::ItemState::Open)
        .count();
    let output = String::from_utf8_lossy(&applied.stdout);
    assert!(
        output.contains(&format!(
            "{} worklist item(s) checked off",
            instrument_ids.len()
        )),
        "{output}"
    );
    assert!(
        output.contains(&format!("{remaining} tracing item(s) remain open")),
        "{output}"
    );
    Ok(())
}
