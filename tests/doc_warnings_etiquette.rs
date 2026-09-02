use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cordial::{
    DOC_WARNINGS_ETIQUETTE, DocWarningRuleId, DocWarningsThresholds, RunAll, Session,
    SessionBuilder, load_cordial_config, parse_doc_compiler_output, scan_crate_doc_warnings,
};
use miette::{IntoDiagnostic, WrapErr};

const CANARY: &str = include_str!("fixtures/quality/doc_warnings/canary.jsonl");

/// Serializes tests that touch the process-wide `CORDIAL_CARGO` env var
/// (`session_writes_checklist_from_injected_cargo` sets it to a fake
/// `cargo`) against tests that depend on it being unset
/// (`dogfood_cordial_has_no_rustdoc_warnings`) -- `cargo test` runs every
/// test in this file as threads of one process, so without this a real
/// scan running concurrently with the injection picks up the fake `cargo`
/// and reports the fixture's canned warnings instead of a real `cargo doc`
/// run against this crate.
static CARGO_ENV_LOCK: Mutex<()> = Mutex::new(());

fn canary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/quality/doc_warnings/canary.jsonl")
}

#[test]
fn parse_canary_keeps_two_unique_rustdoc_warnings() {
    cordial::init_tracing();
    let crate_root = PathBuf::from("/workspace");
    let records = parse_doc_compiler_output(CANARY, &crate_root);
    assert_eq!(records.len(), 2, "{records:?}");
    assert!(
        records
            .iter()
            .all(|record| record.rule_id == DocWarningRuleId::Warning001)
    );
    assert!(records.iter().any(|record| {
        record.line == 3
            && record.context == "rustdoc::broken_intra_doc_links"
            && record.snippet.contains("Nope")
    }));
    assert!(records.iter().any(|record| {
        record.line == 7
            && record.context == "rustdoc::unescaped_backticks"
            && record.snippet.contains("backtick")
    }));
}

#[test]
fn parse_drops_rustc_lints_summaries_and_errors_without_rustdoc_code() {
    cordial::init_tracing();
    let output = "\
error: expected `;`
 --> src/lib.rs:1:1

warning: missing documentation for a struct
 --> src/lib.rs:2:1

warning[unused_variables]: unused variable: `x`
 --> src/lib.rs:4:5

warning: 3 warnings emitted
";
    let records = parse_doc_compiler_output(output, &PathBuf::from("/workspace"));
    assert!(records.is_empty(), "{records:?}");
}

/// `cargo doc`'s own JSON diagnostics report `file_name` relative to the
/// *workspace* root even when `cargo` is invoked with `current_dir` set
/// to one member's own directory -- confirmed against a real `cargo doc`
/// run in a real multi-member workspace, not assumed. Joining a path
/// like `crates/member/src/lib.rs` against the *member's own* root
/// (rather than the workspace root `scan_crate_doc_warnings` now takes
/// as a separate `resolve_root` parameter) would double-prepend it into
/// `crates/member/crates/member/src/lib.rs` -- the real bug this
/// regresses.
#[test]
fn resolves_a_workspace_relative_diagnostic_path_against_the_given_root() {
    cordial::init_tracing();
    let output = "\
warning[rustdoc::broken_intra_doc_links]: unresolved link to `Foo`
 --> crates/member/src/lib.rs:3:11
";
    let workspace_root = PathBuf::from("/workspace");
    let records = parse_doc_compiler_output(output, &workspace_root);
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(
        records[0].file,
        PathBuf::from("/workspace/crates/member/src/lib.rs"),
        "joined once against the given root, not doubled: {:?}",
        records[0].file
    );
}

#[test]
fn parse_human_rustdoc_warning() {
    cordial::init_tracing();
    let output = "\
warning[rustdoc::broken_intra_doc_links]: unresolved link to `Foo`
 --> src/lib.rs:12:11
";
    let records = parse_doc_compiler_output(output, &PathBuf::from("/workspace"));
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].line, 12);
    assert_eq!(records[0].context, "rustdoc::broken_intra_doc_links");
    assert!(records[0].snippet.contains("Foo"));
}

#[test]
fn skip_crates_does_not_invoke_cargo() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"skip_me\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[doc_warnings]\nskip_crates = [\"skip_me\"]\n",
    )
    .into_diagnostic()?;
    let policy = load_cordial_config(fixture.path(), fixture.path());
    let records = scan_crate_doc_warnings(
        fixture.path(),
        fixture.path(),
        "skip_me",
        policy.doc_warnings(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(records.is_empty());
    Ok(())
}

#[test]
fn scan_skips_directory_without_manifest() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let records = scan_crate_doc_warnings(
        fixture.path(),
        fixture.path(),
        "ghost",
        load_cordial_config(fixture.path(), fixture.path()).doc_warnings(),
    )
    .into_diagnostic()
    .wrap_err("scan")?;
    assert!(records.is_empty());
    Ok(())
}

#[cfg(unix)]
#[test]
fn session_writes_checklist_from_injected_cargo() -> miette::Result<()> {
    cordial::init_tracing();
    use std::os::unix::fs::PermissionsExt;
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("canary");
    fs::create_dir_all(crate_root.join("src")).into_diagnostic()?;
    fs::write(
        crate_root.join("Cargo.toml"),
        "[package]\nname = \"canary\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(
        crate_root.join("src/lib.rs"),
        "//! Crate.\n/// See [`Nope`].\npub fn ready() {}\n",
    )
    .into_diagnostic()?;

    let fake = fixture.path().join("fake-cargo");
    let script = format!("#!/bin/sh\ncat \"{}\"\n", canary_path().display());
    fs::write(&fake, script).into_diagnostic()?;
    let mut perms = fs::metadata(&fake).into_diagnostic()?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake, perms).into_diagnostic()?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;

    let guard = CARGO_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = std::env::var_os("CORDIAL_CARGO");
    // SAFETY: test process owns this env var for the duration of the session run.
    unsafe {
        std::env::set_var("CORDIAL_CARGO", &fake);
    }
    let outcome = {
        let session = SessionBuilder::new(&crate_root)
            .with_store_root(store.path())
            .register(&DOC_WARNINGS_ETIQUETTE)
            .build();
        session.run(&RunAll)
    };
    match previous {
        Some(value) => unsafe {
            std::env::set_var("CORDIAL_CARGO", value);
        },
        None => unsafe {
            std::env::remove_var("CORDIAL_CARGO");
        },
    }
    drop(guard);
    let outcome = outcome.into_diagnostic().wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 2);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("doc-warnings.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("DOC-WARNING-001"));
    assert!(csv.contains("rustdoc::broken_intra_doc_links"));
    assert!(csv.contains("Nope"));
    assert!(csv.contains("unescaped_backticks"));

    let checklist = fs::read_to_string(findings_dir.join("doc-warnings.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 2"));
    assert!(checklist.contains("RUSTDOCFLAGS"));
    assert!(checklist.contains("`src/lib.rs:3`") || checklist.contains("src/lib.rs:3"));

    let summary = fs::read_to_string(findings_dir.join("doc-warnings-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("**2** rustdoc"));
    Ok(())
}

#[test]
fn dogfood_cordial_has_no_rustdoc_warnings() -> miette::Result<()> {
    cordial::init_tracing();
    let guard = CARGO_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let records = scan_crate_doc_warnings(root, root, "cordial", &DocWarningsThresholds::default())
        .into_diagnostic()
        .wrap_err("scan cordial")?;
    drop(guard);
    assert!(
        records.is_empty(),
        "cordial cargo doc should be clean of rustdoc::* diagnostics: {records:#?}"
    );
    Ok(())
}
