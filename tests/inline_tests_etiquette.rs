use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    INLINE_TESTS_ETIQUETTE, InlineTestRuleId, RunAll, Session, SessionBuilder,
    scan_inline_tests_rust_source,
};

const INLINE_SRC: &str = r#"
pub fn live() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden() {
        live();
    }
}

#[test]
fn free_test() {}

#[cfg(test)]
fn helper() {}

#[cfg(not(test))]
fn production_only() {}
"#;

#[test]
fn scan_inline_tests_skips_inner_tests_of_cfg_mod() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("lib.rs");
    fs::write(&file, INLINE_SRC)
        .into_diagnostic()
        .wrap_err("write sample")?;

    let findings = scan_inline_tests_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;

    let mods = findings
        .iter()
        .filter(|record| record.rule_id() == InlineTestRuleId::Mod001)
        .count();
    let cfgs = findings
        .iter()
        .filter(|record| record.rule_id() == InlineTestRuleId::Cfg001)
        .count();
    let fns = findings
        .iter()
        .filter(|record| record.rule_id() == InlineTestRuleId::Fn001)
        .count();
    assert_eq!(mods, 1);
    assert_eq!(cfgs, 1);
    assert_eq!(fns, 1);
    assert!(
        findings
            .iter()
            .any(|record| record.snippet().contains("mod tests"))
    );
    assert!(
        findings
            .iter()
            .any(|record| record.snippet().contains("fn helper"))
    );
    assert!(
        findings
            .iter()
            .any(|record| record.snippet().contains("fn free_test"))
    );
    assert!(
        !findings
            .iter()
            .any(|record| record.snippet().contains("hidden"))
    );
    assert!(
        !findings
            .iter()
            .any(|record| record.snippet().contains("production_only"))
    );
    Ok(())
}

#[test]
fn inline_tests_ignore_crate_tests_directory() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::create_dir_all(fixture.path().join("tests"))
        .into_diagnostic()
        .wrap_err("tests")?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n")
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(
        fixture.path().join("tests/ok.rs"),
        "#[test]\nfn integration() {}\n",
    )
    .into_diagnostic()
    .wrap_err("it")?;

    let records = cordial::scan_crate_inline_tests(fixture.path())
        .into_diagnostic()
        .wrap_err("scan crate")?;
    assert!(records.is_empty());
    Ok(())
}

#[test]
fn inline_tests_etiquette_writes_checklist() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), INLINE_SRC)
        .into_diagnostic()
        .wrap_err("write fixture")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&INLINE_TESTS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 3);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("inline-tests.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("INLINE-TEST-MOD"));
    assert!(csv.contains("INLINE-TEST-FN"));
    assert!(csv.contains("INLINE-TEST-CFG"));

    let checklist = fs::read_to_string(findings_dir.join("inline-tests.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 3"));
    Ok(())
}
