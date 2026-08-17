use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{PANICS_ETIQUETTE, RunAll, Session, SessionBuilder};

#[test]
fn exception_patch_suppresses_matching_panic_finding() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub fn boom() {
    panic!("kaboom");
}

pub fn fragile() -> u32 {
    Some(1).expect("missing")
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let slug = cordial::project_slug_from_path(fixture.path());
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
        .register(&PANICS_ETIQUETTE)
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

#[test]
fn quality_patches_alias_suppresses_matching_finding() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"
pub fn boom() {
    panic!("kaboom");
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let slug = cordial::project_slug_from_path(fixture.path());
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
        .register(&PANICS_ETIQUETTE)
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
