//! Antipattern policy: workspace versions, error-field and skip-path rules, the reports, and documented exceptions.

use cordial::{
    ANTIPATTERNS_ETIQUETTE, AntipatternRuleId, ErrorFieldStrategy, RunAll, Session, SessionBuilder,
    StaticRefStrategy, scan_antipatterns_rust_source_with_skip_paths,
    scan_antipatterns_rust_source_with_strategies,
};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::{Path, PathBuf};

#[path = "support/antipatterns_crate.rs"]
mod antipatterns_crate;

#[path = "support/antipatterns_fixture.rs"]
mod antipatterns_fixture;

use antipatterns_crate::crate_antipatterns;
use antipatterns_fixture::fixture;

#[test]
fn workspace_version_findings_refresh_after_manifest_edit() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir().into_diagnostic()?;
    let member = workspace.path().join("member");
    fs::create_dir_all(member.join("src")).into_diagnostic()?;
    fs::write(member.join("src/lib.rs"), "pub struct Example;\n").into_diagnostic()?;
    fs::write(
        workspace.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"member\"]\n[workspace.package]\nversion = \"0.1.0\"\n",
    )
    .into_diagnostic()?;
    let manifest = member.join("Cargo.toml");
    fs::write(
        &manifest,
        "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    let first = crate_antipatterns(&member, "member", workspace.path())?;
    assert!(
        first
            .iter()
            .any(|item| item.rule_id() == AntipatternRuleId::VersionInMember001)
    );

    fs::write(
        &manifest,
        "[package]\nname = \"member\"\nversion.workspace = true\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    let second = crate_antipatterns(&member, "member", workspace.path())?;
    assert!(
        second
            .iter()
            .all(|item| item.rule_id() != AntipatternRuleId::VersionInMember001)
    );
    Ok(())
}

#[test]
fn borrowed_error_fields_do_not_disable_general_static_ref_rule() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct ErrorType { file: &'static str }
impl std::error::Error for ErrorType {}
struct Ordinary { file: &'static str }
struct UnrelatedKind { file: &'static str }
"#;
    let root = Path::new("/fixture/src");
    let file = root.join("lib.rs");
    let borrowed = scan_antipatterns_rust_source_with_strategies(
        source,
        &file,
        root,
        root,
        StaticRefStrategy::String,
        ErrorFieldStrategy::Borrowed,
    )
    .into_diagnostic()?;
    assert!(
        !borrowed
            .iter()
            .any(|finding| finding.context().contains("ErrorType"))
    );
    assert!(
        borrowed
            .iter()
            .any(|finding| finding.context().contains("Ordinary"))
    );
    assert!(
        borrowed
            .iter()
            .any(|finding| finding.context().contains("UnrelatedKind"))
    );
    let owned = scan_antipatterns_rust_source_with_strategies(
        source,
        &file,
        root,
        root,
        StaticRefStrategy::String,
        ErrorFieldStrategy::Owned,
    )
    .into_diagnostic()?;
    assert!(
        owned
            .iter()
            .any(|finding| finding.context().contains("ErrorType"))
    );
    Ok(())
}

#[test]
fn skip_paths_exempt_one_rule_on_a_matched_path_only() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
struct Gallery { value: &'static str }
fn gallery_proof(_context: &str) {}
"#;
    let root = Path::new("/fixture/src");
    let file = root.join("gallery.rs");

    let mut skip_paths = std::collections::HashMap::new();
    skip_paths.insert(
        AntipatternRuleId::StructStaticRef001.as_str().to_string(),
        vec!["gallery.rs".to_string()],
    );
    let scoped = scan_antipatterns_rust_source_with_skip_paths(
        source,
        &file,
        root,
        root,
        StaticRefStrategy::String,
        ErrorFieldStrategy::Owned,
        &skip_paths,
    )
    .into_diagnostic()?;
    assert!(
        !scoped
            .iter()
            .any(|finding| finding.rule_id() == AntipatternRuleId::StructStaticRef001),
        "{scoped:?}"
    );
    assert!(
        scoped
            .iter()
            .any(|finding| finding.rule_id() == AntipatternRuleId::UnusedUnderscoreArg001),
        "skip_paths must not exempt a different rule on the same file: {scoped:?}"
    );

    let other_file = root.join("elsewhere.rs");
    let unscoped = scan_antipatterns_rust_source_with_skip_paths(
        source,
        &other_file,
        root,
        root,
        StaticRefStrategy::String,
        ErrorFieldStrategy::Owned,
        &skip_paths,
    )
    .into_diagnostic()?;
    assert!(
        unscoped
            .iter()
            .any(|finding| finding.rule_id() == AntipatternRuleId::StructStaticRef001),
        "skip_paths must not exempt an unmatched path: {unscoped:?}"
    );
    Ok(())
}

#[test]
fn antipatterns_etiquette_emits_reports() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/parity/workspaces/box_dyn_error");
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(&workspace)
        .with_store_root(store.path())
        .register(&*ANTIPATTERNS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(
        outcome
            .findings()
            .filter(|f| f.rule().category() == "antipatterns")
            .count(),
        4
    );

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("antipatterns.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("ANTIPATTERN-BOX-DYN-ERROR-001"));

    let checklist = fs::read_to_string(findings_dir.join("antipatterns.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 4"));
    assert!(checklist.contains("accepts"));

    let summary = fs::read_to_string(findings_dir.join("antipatterns-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("box_dyn_error **4**"));
    Ok(())
}

#[test]
fn documented_exceptions_suppress_checklist_items() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(workspace.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        workspace.path().join("src/lib.rs"),
        fixture("unused_underscore_args.rs")?,
    )
    .into_diagnostic()
    .wrap_err("write fixture")?;
    fs::write(
        workspace.path().join("Cargo.toml"),
        r#"[workspace]
members = ["."]

[workspace.package]
version = "0.1.0"
edition = "2024"

[package]
name = "fixture"
version = { workspace = true }
edition = { workspace = true }
"#,
    )
    .into_diagnostic()
    .wrap_err("write manifest")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    fs::create_dir_all(store.path().join("exceptions/antipatterns"))
        .into_diagnostic()
        .wrap_err("exceptions dir")?;
    fs::write(
        store.path().join("exceptions/antipatterns/fixture.json"),
        r#"[
  {
    "file": "src/lib.rs",
    "context": "Bot::handle",
    "reason": "fixture exception"
  }
]"#,
    )
    .into_diagnostic()
    .wrap_err("write exception")?;

    let session = SessionBuilder::new(workspace.path())
        .with_store_root(store.path())
        .register(&*ANTIPATTERNS_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let open = outcome
        .findings()
        .filter(|f| {
            f.rule().category() == "antipatterns" && f.disposition() == cordial::Disposition::Open
        })
        .count();
    assert_eq!(open, 4);

    let checklist = fs::read_to_string(store.path().join("findings/antipatterns.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("Documented exceptions"));
    assert!(checklist.contains("fixture exception"));
    Ok(())
}
