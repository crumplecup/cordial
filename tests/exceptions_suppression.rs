//! An exception patch suppresses the finding it names, including through the quality alias.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{PANICS_ETIQUETTE, RunAll, Session, SessionBuilder, VISIBILITY_ETIQUETTE};

#[test]
fn exception_patch_suppresses_matching_panic_finding() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/exceptions_patch.rs"),
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let slug = "fixture";
    let exceptions_dir = store.path().join("exceptions").join("panics");
    fs::create_dir_all(&exceptions_dir)
        .into_diagnostic()
        .wrap_err("exceptions dir")?;
    fs::write(
        exceptions_dir.join(format!("{slug}.json")),
        r#"[
  {
    "file": "src/lib.rs",
    "rule_id": "PANIC-SOURCE-PANIC",
    "reason": "Demo panic is intentional"
  }
]"#,
    )
    .into_diagnostic()
    .wrap_err("write exception patch")?;

    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert_eq!(findings.len(), 2);

    let open = findings
        .iter()
        .filter(|f| f.disposition() == cordial::Disposition::Open)
        .count();
    let suppressed = findings
        .iter()
        .filter(|f| f.disposition() == cordial::Disposition::Suppressed)
        .count();
    assert_eq!(open, 1);
    assert_eq!(suppressed, 1);

    let checklist = fs::read_to_string(store.path().join("findings/panics.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 1"));
    assert!(checklist.contains("Documented exceptions"));
    Ok(())
}

/// Regression test for a real bug: `VisibilityFinding::emit` only emitted
/// `module_path`, never `context` -- the field name every other
/// etiquette's own `Finding::emit` uses for exactly this match. Since
/// `ExceptionEntry::matches` looks up `context` by that literal name, a
/// `--context`-scoped exception could never match a visibility finding at
/// all before the fix, even a `file` + `rule_id`-correct one.
#[test]
fn exception_patch_with_context_suppresses_matching_visibility_finding() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub mod widgets;\npub fn root_item() {}\n",
    )
    .into_diagnostic()
    .wrap_err("write lib.rs")?;
    fs::write(
        fixture.path().join("src/widgets.rs"),
        "pub fn widget() {}\n",
    )
    .into_diagnostic()
    .wrap_err("write widgets.rs")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let slug = "fixture";
    let exceptions_dir = store.path().join("exceptions").join("visibility");
    fs::create_dir_all(&exceptions_dir)
        .into_diagnostic()
        .wrap_err("exceptions dir")?;
    fs::write(
        exceptions_dir.join(format!("{slug}.json")),
        r#"[
  {
    "file": "src/widgets.rs",
    "rule_id": "VIS-MOD-THIN-001",
    "context": "crate::widgets",
    "reason": "Demo: context-scoped visibility exception"
  }
]"#,
    )
    .into_diagnostic()
    .wrap_err("write exception patch")?;

    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*VISIBILITY_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let dispositions: Vec<_> = outcome
        .findings()
        .filter(|f| f.rule().id() == "VIS-MOD-THIN-001")
        .map(|f| f.disposition())
        .collect();
    assert_eq!(dispositions.len(), 1, "{dispositions:?}");
    assert_eq!(
        dispositions[0],
        cordial::Disposition::Suppressed,
        "a context-scoped exception naming crate::widgets must match its \
         own VIS-MOD-THIN-001 finding: {dispositions:?}"
    );
    Ok(())
}

#[test]
fn quality_patches_alias_suppresses_matching_finding() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/exceptions_alias.rs"),
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let slug = "fixture";
    let patches_dir = store.path().join("quality").join("patches").join("panics");
    fs::create_dir_all(&patches_dir)
        .into_diagnostic()
        .wrap_err("patches dir")?;
    fs::write(
        patches_dir.join(format!("{slug}.json")),
        r#"[
  {
    "file": "src/lib.rs",
    "rule_id": "PANIC-SOURCE-PANIC",
    "reason": "Demo panic is intentional"
  }
]"#,
    )
    .into_diagnostic()
    .wrap_err("write quality patch")?;

    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].disposition(), cordial::Disposition::Suppressed);
    Ok(())
}
