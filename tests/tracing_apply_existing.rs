//! Existing annotations: rewritten to the recipe, custom arguments kept, line endings preserved, no duplicates.

use cordial::run_tracing_instrument_apply;
use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

#[path = "support/tracing_apply_checklist.rs"]
mod tracing_apply_checklist;

#[path = "support/tracing_apply_crate.rs"]
mod tracing_apply_crate;

use tracing_apply_crate::{checklist_for, write_apply_fixture};

#[test]
fn apply_rewrites_existing_instrument_to_recipe() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub struct Store {
    root: String,
}

impl Store {
    #[tracing::instrument]
    pub fn cache_dir(&self) -> String {
        format!("{}/cache", self.root)
    }
}
"#,
        &checklist_for(&[("Store::cache_dir", 8)]),
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 1);
    assert_eq!(summary.skipped_existing(), 0);

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[tracing::instrument(level = \"trace\", skip(self))]"),
        "{updated}"
    );
    assert!(
        !updated.contains("#[tracing::instrument]"),
        "bare default-info attr should be replaced: {updated}"
    );
    assert!(!updated.contains("use tracing::instrument;"));
    Ok(())
}

#[test]
fn apply_preserves_custom_instrument_arguments_and_comments() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"#[tracing::instrument(
    level = "trace",
    name = "custom, operation)",
    parent = None,
    target = "my_app",
    skip_all,
    fields(marker = "keep, )", path = %path),
    ret(level = "debug"), // deliberate return recording
)]
pub fn load_session_config(path: &str) -> Result<(), std::io::Error> { Ok(()) }
"#;
    let fixture = write_apply_fixture(source, &checklist_for(&[("load_session_config", 1)]))?;
    let first = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()?;
    assert_eq!(first.changed_functions(), 1, "{first:?}");
    let expected = source.replace(")]\npub fn", " err(level = \"warn\"))]\npub fn");
    let manifest = fixture
        .src
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| miette::miette!("fixture crate root missing"))?
        .join("Cargo.toml");
    let package = fs::read_to_string(&manifest).into_diagnostic()?;
    fs::write(
        &manifest,
        format!("{package}\n[dependencies]\ntracing = \"0.1\"\n"),
    )
    .into_diagnostic()?;
    let compiled = std::process::Command::new("cargo")
        .args(["check", "--offline", "--quiet", "--manifest-path"])
        .arg(&manifest)
        .output()
        .into_diagnostic()?;
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert_eq!(
        fs::read_to_string(&fixture.src).into_diagnostic()?,
        expected
    );
    let second = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()?;
    assert_eq!(second.changed_functions(), 0, "{second:?}");
    assert_eq!(
        fs::read_to_string(&fixture.src).into_diagnostic()?,
        expected
    );
    Ok(())
}

#[test]
fn apply_merges_lists_without_replacing_custom_fields_or_error_modes() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"#[tracing::instrument(level = "warn", skip(report), fields(marker = "keep", name = %name), err(Debug, level = "error"))]
pub fn scan_tree(name: &str, path: &str, report: &str) -> Result<(), std::io::Error> { Ok(()) }
"#;
    let fixture = write_apply_fixture(source, &checklist_for(&[("scan_tree", 1)]))?;
    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()?;
    assert_eq!(summary.changed_functions(), 1, "{summary:?}");
    let expected = source
        .replace("level = \"warn\"", "level = \"debug\"")
        .replace("skip(report)", "skip(report, path)");
    assert_eq!(
        fs::read_to_string(&fixture.src).into_diagnostic()?,
        expected
    );
    Ok(())
}

#[test]
fn apply_preserves_crlf_and_missing_final_newline() -> miette::Result<()> {
    cordial::init_tracing();
    let source = "#[tracing::instrument(level = \"trace\", skip_all)]\r\npub fn load_session_config() -> Result<(), std::io::Error> { Ok(()) }";
    let fixture = write_apply_fixture(source, &checklist_for(&[("load_session_config", 1)]))?;
    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()?;
    assert_eq!(summary.changed_functions(), 1, "{summary:?}");
    assert_eq!(
        fs::read_to_string(&fixture.src).into_diagnostic()?,
        source.replace("skip_all)", "skip_all, err(level = \"warn\"))")
    );
    Ok(())
}

#[test]
fn apply_changes_only_the_target_cfg_declaration() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"#[cfg(feature = "alternate")]
#[tracing::instrument(level = "trace", skip_all)]
pub fn choose() -> Result<(), std::io::Error> { Ok(()) }
#[cfg(not(feature = "alternate"))]
#[tracing::instrument(level = "trace", skip_all, err(level = "error"))]
pub fn choose() -> Result<(), std::io::Error> { Ok(()) }
"#;
    let fixture = write_apply_fixture(source, &checklist_for(&[("choose", 1)]))?;
    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()?;
    assert_eq!(summary.changed_functions(), 1, "{summary:?}");
    assert_eq!(
        fs::read_to_string(&fixture.src).into_diagnostic()?,
        source.replacen("skip_all)]", "skip_all, err(level = \"warn\"))]", 1)
    );
    Ok(())
}

#[test]
fn apply_leaves_ambiguous_instrument_annotations_unresolved() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"#[tracing::instrument(level = "trace", skip_all)]
#[tracing::instrument(level = "debug", skip_all)]
pub fn load_session_config() -> Result<(), std::io::Error> { Ok(()) }
"#;
    let fixture = write_apply_fixture(source, &checklist_for(&[("load_session_config", 1)]))?;
    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()?;
    assert_eq!(summary.changed_functions(), 0, "{summary:?}");
    assert_eq!(summary.unresolved(), 1, "{summary:?}");
    assert_eq!(fs::read_to_string(&fixture.src).into_diagnostic()?, source);
    Ok(())
}

#[test]
fn apply_rejects_stale_cfg_identity_and_mixed_line_endings() -> miette::Result<()> {
    cordial::init_tracing();
    let sources = [
        "#[cfg(feature = \"other\")]\n#[tracing::instrument(level = \"trace\")]\npub fn choose() -> Result<(), std::io::Error> { Ok(()) }\n#[cfg(not(feature = \"other\"))]\n#[tracing::instrument(level = \"trace\")]\npub fn choose() -> Result<(), std::io::Error> { Ok(()) }\n",
        "#[tracing::instrument(level = \"trace\")]\r\npub fn choose() -> Result<(), std::io::Error> { Ok(()) }\n",
    ];
    for source in sources {
        let fixture = write_apply_fixture(source, &checklist_for(&[("choose", 99)]))?;
        let summary =
            run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
                .into_diagnostic()?;
        assert_eq!(summary.unresolved(), 1, "{summary:?}");
        assert_eq!(summary.changed_functions(), 0, "{summary:?}");
        assert_eq!(fs::read_to_string(&fixture.src).into_diagnostic()?, source);
    }
    Ok(())
}

#[test]
fn apply_skips_when_recipe_already_present() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
use tracing::instrument;

#[instrument(level = "info", err(level = "warn"))]
pub fn load_session_config() -> Result<(), String> {
    Ok(())
}
"#,
        &checklist_for(&[("load_session_config", 5)]),
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 0);
    assert_eq!(summary.skipped_existing(), 1);
    Ok(())
}

#[test]
fn apply_dedupes_multiple_checklist_rows_for_one_fn() -> miette::Result<()> {
    cordial::init_tracing();
    let mut checklist = checklist_for(&[("load_session_config", 2)]);
    checklist.push_str("- [ ] `load_session_config` — `src/lib.rs:2` (pub)\n");
    let fixture = write_apply_fixture(
        r#"
pub fn load_session_config() -> Result<(), String> {
    Ok(())
}
"#,
        &checklist,
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 1);

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    let count = updated.matches("#[instrument(").count();
    assert_eq!(count, 1, "{updated}");
    Ok(())
}
