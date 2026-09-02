use std::fs;
use std::path::PathBuf;

#[cfg(feature = "verus_ir")]
use cordial::scan_crate_verus_ir;
use cordial::{
    RunAll, Session, SessionBuilder, VERUS_WARNINGS_ETIQUETTE, VerusWarningRuleId,
    crate_is_verus_target, parse_verus_compiler_output, scan_crate_verus_warnings,
};
use miette::{IntoDiagnostic, WrapErr};

const CANARY: &str = include_str!("fixtures/quality/verus_warnings/canary.stderr");

fn canary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/quality/verus_warnings/canary.stderr")
}

#[test]
fn parse_amenable_canary_keeps_two_unique_warnings() {
    cordial::init_tracing();
    let crate_root = PathBuf::from("/workspace");
    let records = parse_verus_compiler_output(CANARY, &crate_root);
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|record| record.rule_id == VerusWarningRuleId::Warning001)
    );
    assert!(
        records
            .iter()
            .any(|record| record.line == 132 && record.snippet.contains("impl_tuple_evidence"))
    );
    assert!(
        records
            .iter()
            .any(|record| record.line == 446 && record.snippet.contains("autoderive Clone"))
    );
}

#[test]
fn parse_drops_summary_lines_and_errors() {
    cordial::init_tracing();
    let output = "\
error: expected `;`
 --> src/lib.rs:1:1

warning: 3 warnings emitted
";
    let records = parse_verus_compiler_output(output, &PathBuf::from("/workspace"));
    assert!(records.is_empty());
}

#[test]
fn crate_named_verus_is_a_target() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("demo_verus");
    fs::create_dir_all(crate_root.join("src")).into_diagnostic()?;
    fs::write(
        crate_root.join("Cargo.toml"),
        "[package]\nname = \"demo_verus\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(crate_root.join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    assert!(crate_is_verus_target(&crate_root));
    Ok(())
}

#[test]
fn crate_with_vstd_dep_is_a_target() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"proofs\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nvstd = \"0.0\"\n",
    )
    .into_diagnostic()?;
    assert!(crate_is_verus_target(fixture.path()));
    Ok(())
}

#[test]
fn ordinary_crate_is_not_a_verus_target() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"plain\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    assert!(!crate_is_verus_target(fixture.path()));
    Ok(())
}

#[test]
fn scan_skips_non_verus_crate_without_invoking_compiler() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"plain\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    let records = scan_crate_verus_warnings(fixture.path())
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(records.is_empty());
    Ok(())
}

/// `missing documentation for a method` at a fully-documented,
/// data-carrying enum's own declaration line is Verus's own synthesized
/// pattern-projection accessor, not a real gap -- see
/// `crate::verus_ir::VerusCrateIr::is_documented_pattern_projection_enum`'s
/// own doc comment. An identical warning pointing at an *undocumented*
/// data-carrying enum is a real gap and still flags.
///
/// Composes `parse_verus_compiler_output` + `scan_crate_verus_ir`
/// directly -- the same two real, pure functions `scan_crate_verus_warnings`'s
/// own `retain_real_warnings` filter calls internally -- rather than
/// going through the real subprocess/`CORDIAL_VERUS` injection path
/// `session_writes_checklist_from_injected_verus` uses below: that path
/// mutates a process-global env var, and two tests doing that
/// concurrently (the default, since `cargo test` runs a file's tests in
/// parallel) genuinely race on which fake binary the other one's
/// subprocess picks up -- confirmed the hard way, not a hypothetical.
#[cfg(feature = "verus_ir")]
#[test]
fn pattern_projection_warning_is_suppressed_only_when_fully_documented() -> miette::Result<()> {
    cordial::init_tracing();

    // Line 6 is `pub enum TransferError {` in both crates below -- every
    // human-writable doc site is present in `documented`, and
    // `NegativeAmount`'s own doc comment is missing in `undocumented`.
    let documented_source = "use verus_builtin_macros::verus;\n\nverus! {\n\n/// Sanitized mirror of a real error type.\npub enum TransferError {\n    /// The transfer amount wasn't positive.\n    NegativeAmount(i64),\n}\n\n}\n";
    let undocumented_source = "use verus_builtin_macros::verus;\n\nverus! {\n\n/// Sanitized mirror of a real error type.\npub enum TransferError {\n    NegativeAmount(i64),\n}\n\n}\n";
    let fake_compiler_output = "warning: missing documentation for a method\n   --> src/lib.rs:6:1\n    |\n6   | pub enum TransferError {\n    |\n\nwarning: 1 warning emitted\n";

    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;

    let documented_root = fixture.path().join("documented_verus");
    fs::create_dir_all(documented_root.join("src")).into_diagnostic()?;
    fs::write(documented_root.join("src/lib.rs"), documented_source).into_diagnostic()?;
    let documented_parsed = parse_verus_compiler_output(fake_compiler_output, &documented_root);
    assert_eq!(documented_parsed.len(), 1, "sanity: the fake output parses");
    let documented_ir = scan_crate_verus_ir(&documented_root)
        .into_diagnostic()
        .wrap_err("scan documented ir")?;
    let documented_kept: Vec<_> = documented_parsed
        .iter()
        .filter(|record| {
            !(record.snippet == "missing documentation for a method"
                && documented_ir.is_documented_pattern_projection_enum(&record.file, record.line))
        })
        .collect();
    assert!(
        documented_kept.is_empty(),
        "fully-documented data-carrying enum should suppress the warning: {documented_kept:?}"
    );

    let undocumented_root = fixture.path().join("undocumented_verus");
    fs::create_dir_all(undocumented_root.join("src")).into_diagnostic()?;
    fs::write(undocumented_root.join("src/lib.rs"), undocumented_source).into_diagnostic()?;
    let undocumented_parsed = parse_verus_compiler_output(fake_compiler_output, &undocumented_root);
    let undocumented_ir = scan_crate_verus_ir(&undocumented_root)
        .into_diagnostic()
        .wrap_err("scan undocumented ir")?;
    let undocumented_kept: Vec<_> = undocumented_parsed
        .iter()
        .filter(|record| {
            !(record.snippet == "missing documentation for a method"
                && undocumented_ir.is_documented_pattern_projection_enum(&record.file, record.line))
        })
        .collect();
    assert_eq!(
        undocumented_kept.len(),
        1,
        "a real doc gap on the data-carrying variant must still flag: {undocumented_kept:?}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn session_writes_checklist_from_injected_verus() -> miette::Result<()> {
    cordial::init_tracing();
    use std::os::unix::fs::PermissionsExt;
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("canary_verus");
    fs::create_dir_all(crate_root.join("src")).into_diagnostic()?;
    fs::write(
        crate_root.join("Cargo.toml"),
        "[package]\nname = \"canary_verus\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(crate_root.join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;

    let fake = fixture.path().join("fake-verus");
    let script = format!("#!/bin/sh\ncat \"{}\"\n", canary_path().display());
    fs::write(&fake, script).into_diagnostic()?;
    let mut perms = fs::metadata(&fake).into_diagnostic()?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake, perms).into_diagnostic()?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;

    let previous = std::env::var_os("CORDIAL_VERUS");
    // SAFETY: test process owns this env var for the duration of the session run.
    unsafe {
        std::env::set_var("CORDIAL_VERUS", &fake);
    }
    let outcome = {
        let session = SessionBuilder::new(&crate_root)
            .with_store_root(store.path())
            .register(&VERUS_WARNINGS_ETIQUETTE)
            .build();
        session.run(&RunAll)
    };
    match previous {
        Some(value) => unsafe {
            std::env::set_var("CORDIAL_VERUS", value);
        },
        None => unsafe {
            std::env::remove_var("CORDIAL_VERUS");
        },
    }
    let outcome = outcome.into_diagnostic().wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 2);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("verus-warnings.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("VERUS-WARNING-001"));
    assert!(csv.contains("impl_tuple_evidence"));
    assert!(csv.contains("autoderive Clone"));

    let checklist = fs::read_to_string(findings_dir.join("verus-warnings.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 2"));
    assert!(checklist.contains("deny-warnings"));

    let summary = fs::read_to_string(findings_dir.join("verus-warnings-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("**2** Verus compiler warnings"));
    Ok(())
}
