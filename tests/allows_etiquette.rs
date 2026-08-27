use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::PathBuf;

use cordial::{
    ALLOWS_ETIQUETTE, AllowRuleId, AllowSiteRecord, RunAll, Session, SessionBuilder,
    scan_allows_rust_source,
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
    cordial::init_tracing();
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
    cordial::init_tracing();
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
    cordial::init_tracing();
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

#[test]
fn scan_allows_rust_source_finds_allow_on_use_items() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("uses.rs");
    fs::write(
        &file,
        r#"
#[allow(unused_imports)]
use std::collections::HashMap;

mod inner {
    #[allow(unused_imports)]
    use std::fs;
}
"#,
    )
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
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(
        findings
            .iter()
            .all(|record| record.snippet.contains("unused_imports"))
    );
    assert!(
        findings
            .iter()
            .any(|record| record.context.ends_with("inner")),
        "{findings:?}"
    );
    Ok(())
}

const VERUS_PRELUDE_WITH_REASON: &str = r#"
use verus_builtin_macros::verus;
#[allow(
    unused_imports,
    reason = "vstd::prelude::* is unused under plain rustc (verus! {} erases real spec content); needed only when the real verus toolchain parses this file directly"
)]
use vstd::prelude::*;
"#;

const VERUS_PRELUDE_WITHOUT_REASON: &str = r#"
use verus_builtin_macros::verus;
#[allow(unused_imports)]
use vstd::prelude::*;
"#;

#[test]
fn reasoned_verus_vstd_allow_is_not_an_action_item() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_inline("verus_reasoned.rs", VERUS_PRELUDE_WITH_REASON)?;
    assert!(
        findings.is_empty(),
        "reasoned Verus prelude allow must drop out of the action list: {findings:?}"
    );
    Ok(())
}

#[test]
fn verus_vstd_allow_without_reason_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_inline("verus_bare.rs", VERUS_PRELUDE_WITHOUT_REASON)?;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].rule_id, AllowRuleId::VerusReason001);
    assert!(findings[0].snippet.contains("unused_imports"));
    Ok(())
}

#[test]
fn verus_allow_empty_reason_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = scan_inline(
        "verus_empty_reason.rs",
        r#"
#[allow(unused_imports, reason = "")]
use vstd::float::FloatBitsProperties;
"#,
    )?;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].rule_id, AllowRuleId::VerusReason001);
    Ok(())
}

#[test]
fn allows_etiquette_emits_verus_reason_finding() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        VERUS_PRELUDE_WITHOUT_REASON,
    )
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
    assert_eq!(outcome.findings().count(), 1);

    let checklist = fs::read_to_string(store.path().join("findings/allows.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("ALLOW-VERUS-REASON-001"));
    assert!(checklist.contains("**Open items:** 1"));
    Ok(())
}

fn scan_inline(name: &str, source: &str) -> miette::Result<Vec<AllowSiteRecord>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join(name);
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write sample")?;
    scan_allows_rust_source(
        &fs::read_to_string(&file).into_diagnostic()?,
        &file,
        fixture.path(),
        fixture.path(),
    )
    .into_diagnostic()
    .wrap_err("scan")
}
