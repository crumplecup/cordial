use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::PathBuf;

use cordial::{
    ALLOWS_ETIQUETTE, AllowRuleId, RunAll, Session, SessionBuilder, scan_allows_rust_source,
};

const ALLOW_ATTRS: &str = r#"#![allow(dead_code)]

#[allow(clippy::too_many_arguments)]
fn many_args(_: u8, _: u8, _: u8, _: u8, _: u8) {}

struct Hidden;

struct Wrapper {
    #[allow(unused)]
    field: u8,
}

mod inner {
    #[allow(unused_variables)]
    fn unused_binding(value: u8) {
        let _ = value;
    }
}

fn clean_fn() {}
"#;

#[test]
fn allows_workspace_fixture_has_four_sites() -> miette::Result<()> {
    let workspace =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/parity/workspaces/allow_attrs");
    let records = cordial::scan_crate_allows(&workspace)
        .into_diagnostic()
        .wrap_err("scan workspace")?;
    assert_eq!(records.len(), 4);
    Ok(())
}

#[test]
fn allows_etiquette_detects_allow_attributes() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), ALLOW_ATTRS)
        .into_diagnostic()
        .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&ALLOWS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 4);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("allows.csv"))
        .into_diagnostic()
        .wrap_err("allows csv")?;
    assert!(csv.contains("ALLOW-ATTR-001"));
    assert!(csv.contains("clippy::too_many_arguments"));

    let checklist = fs::read_to_string(findings_dir.join("allows.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 4"));
    assert!(checklist.contains("inner::unused_binding"));

    let summary = fs::read_to_string(findings_dir.join("allows-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("**4** sites"));
    Ok(())
}

#[test]
fn scan_allows_rust_source_finds_four_sites() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("allow_attrs.rs");
    fs::write(&file, ALLOW_ATTRS)
        .into_diagnostic()
        .wrap_err("write sample")?;

    let findings = scan_allows_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert_eq!(
        findings
            .iter()
            .filter(|record| record.rule_id == AllowRuleId::Attr001)
            .count(),
        4
    );
    assert!(
        findings
            .iter()
            .any(|record| record.context.contains("many_args"))
    );
    assert!(
        !findings
            .iter()
            .any(|record| record.context.contains("clean_fn"))
    );
    Ok(())
}
