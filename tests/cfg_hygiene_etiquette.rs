use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    CFG_HYGIENE_ETIQUETTE, CfgHygieneThresholds, MapFindingSink, RunAll, Session, SessionBuilder,
};

const SOURCE_WITH_UNDECLARED_AND_BUILTIN: &str = r#"
#[cfg(totally_undeclared_name)]
fn only_under_undeclared() {}

#[cfg(test)]
fn only_under_test() {}
"#;

#[test]
fn scan_cfg_hygiene_rust_source_extracts_names_through_combinators() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let file = fixture.path().join("lib.rs");
    let source = r#"
#[cfg(kani)]
fn a() {}

#[cfg(not(creusot))]
fn b() {}

#[cfg(all(feature = "x", not(test)))]
fn c() {}

#[cfg_attr(kani, kani::requires(true))]
fn d() {}
"#;
    fs::write(&file, source)
        .into_diagnostic()
        .wrap_err("write source")?;

    let occurrences =
        cordial::scan_cfg_hygiene_rust_source(source, &file, fixture.path(), fixture.path())
            .into_diagnostic()
            .wrap_err("scan")?;
    let names: Vec<_> = occurrences.iter().map(|o| o.name().as_str()).collect();

    assert!(names.contains(&"kani"), "bare cfg(kani): {names:?}");
    assert!(
        names.contains(&"creusot"),
        "cfg(not(creusot)) should surface creusot: {names:?}"
    );
    assert!(
        names.contains(&"feature"),
        "cfg(all(feature = \"x\", ...)) should surface feature: {names:?}"
    );
    assert!(
        names.contains(&"test"),
        "cfg(all(..., not(test))) should surface test: {names:?}"
    );
    assert!(
        names.iter().filter(|n| **n == "kani").count() == 2,
        "cfg_attr(kani, ...) predicate should surface kani, its splice target must not: {names:?}"
    );
    assert!(
        !names.contains(&"requires"),
        "cfg_attr's spliced-in attribute is not a cfg predicate: {names:?}"
    );
    Ok(())
}

#[test]
fn cfg_hygiene_etiquette_flags_undeclared_but_not_builtin() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        SOURCE_WITH_UNDECLARED_AND_BUILTIN,
    )
    .into_diagnostic()
    .wrap_err("write lib")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&CFG_HYGIENE_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();

    let unexpected: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule().id() == "UNEXPECTED-CFG-001")
        .collect();
    assert!(
        !unexpected.is_empty(),
        "expected an UNEXPECTED-CFG-001 finding for totally_undeclared_name"
    );
    let mut sink = MapFindingSink::default();
    unexpected[0].emit(&mut sink);
    assert!(
        sink.fields
            .iter()
            .any(|(k, v)| k == "cfg_name" && v == "totally_undeclared_name"),
        "flagged finding should name totally_undeclared_name: {:?}",
        sink.fields
    );
    assert!(
        findings.iter().all(|finding| {
            let mut s = MapFindingSink::default();
            finding.emit(&mut s);
            !s.fields.iter().any(|(k, v)| k == "cfg_name" && v == "test")
        }),
        "cfg(test) is Cargo-injected and must never be flagged"
    );

    let findings_dir = store.path().join("findings");
    assert!(findings_dir.join("cfg-hygiene.csv").is_file());
    assert!(findings_dir.join("cfg-hygiene.checklist.md").is_file());
    assert!(findings_dir.join("cfg-hygiene-summary.md").is_file());
    Ok(())
}

#[test]
fn cfg_hygiene_etiquette_respects_build_rs_declarations() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"#[cfg(my_custom_cfg)]
fn gated() {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write lib")?;
    fs::write(
        fixture.path().join("build.rs"),
        r#"fn main() {
    println!("cargo::rustc-check-cfg=cfg(my_custom_cfg)");
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write build.rs")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&CFG_HYGIENE_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert!(
        findings
            .iter()
            .all(|finding| finding.rule().id() != "UNEXPECTED-CFG-001"),
        "build.rs-declared my_custom_cfg must not be flagged"
    );
    Ok(())
}

#[test]
fn cfg_hygiene_etiquette_flags_verifier_mismatch_but_not_own_identity() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"my_kani_crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .into_diagnostic()
    .wrap_err("write manifest")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        r#"#[cfg(kani)]
fn own_verifier() {}

#[cfg(creusot)]
fn wrong_verifier() {}
"#,
    )
    .into_diagnostic()
    .wrap_err("write lib")?;
    fs::write(
        fixture.path().join("build.rs"),
        r#"fn main() {
    println!("cargo::rustc-check-cfg=cfg(kani)");
    println!("cargo::rustc-check-cfg=cfg(creusot)");
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write build.rs")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[cfg_hygiene.crate_verifier]\nmy_kani_crate = \"kani\"\nmy_creusot_crate = \"creusot\"\n",
    )
    .into_diagnostic()
    .wrap_err("write cordial.toml")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&CFG_HYGIENE_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let findings: Vec<_> = outcome.findings().collect();

    let mismatches: Vec<_> = findings
        .iter()
        .filter(|finding| finding.rule().id() == "CFG-VERIFIER-MISMATCH-001")
        .collect();
    assert_eq!(
        mismatches.len(),
        1,
        "expected exactly one verifier-mismatch finding (for creusot), got {} findings",
        mismatches.len()
    );
    let mut sink = MapFindingSink::default();
    mismatches[0].emit(&mut sink);
    assert!(
        sink.fields
            .iter()
            .any(|(k, v)| k == "cfg_name" && v == "creusot"),
        "the mismatch finding should name creusot: {:?}",
        sink.fields
    );
    Ok(())
}

#[test]
fn cfg_hygiene_etiquette_summary_and_checklist_group_by_each_finding_own_crate()
-> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/alpha\", \"crates/beta\"]\nresolver = \"2\"\n",
    )
    .into_diagnostic()
    .wrap_err("workspace manifest")?;
    write_member(
        fixture.path(),
        "crates/alpha",
        "alpha",
        "#[cfg(alpha_only_cfg)]\nfn a() {}\n",
    )?;
    write_member(
        fixture.path(),
        "crates/beta",
        "beta",
        "#[cfg(beta_only_cfg)]\nfn b() {}\n",
    )?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&CFG_HYGIENE_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let checklist = fs::read_to_string(store.path().join("findings/cfg-hygiene.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("## `alpha`") && checklist.contains("alpha_only_cfg"),
        "alpha's own finding should appear under its own heading: {checklist}"
    );
    assert!(
        checklist.contains("## `beta`") && checklist.contains("beta_only_cfg"),
        "beta's own finding should appear under its own heading: {checklist}"
    );

    let summary = fs::read_to_string(store.path().join("findings/cfg-hygiene-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(
        summary.contains("| `alpha` | 1 | 0 |"),
        "alpha should get its own summary row: {summary}"
    );
    assert!(
        summary.contains("| `beta` | 1 | 0 |"),
        "beta should get its own summary row: {summary}"
    );
    assert!(summary.contains("Workspace totals: **2**"));
    Ok(())
}

fn write_member(
    root: &std::path::Path,
    rel: &str,
    name: &str,
    lib_body: &str,
) -> miette::Result<()> {
    let crate_root = root.join(rel);
    fs::create_dir_all(crate_root.join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        crate_root.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
    )
    .into_diagnostic()
    .wrap_err("member manifest")?;
    fs::write(crate_root.join("src/lib.rs"), lib_body)
        .into_diagnostic()
        .wrap_err("lib rs")?;
    Ok(())
}

#[test]
fn cfg_hygiene_default_thresholds() {
    cordial::init_tracing();
    let thresholds = CfgHygieneThresholds::default();
    assert!(thresholds.extra_known_names().is_empty());
    assert!(thresholds.crate_verifier().is_empty());
}
