use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cordial::{
    FEATURE_WARNINGS_ETIQUETTE, FeatureSet, FeatureWarningRuleId, Gate, RunAll, Session,
    SessionBuilder, parse_cargo_hack_output, records_from_run, suggest_gate,
};
use miette::{IntoDiagnostic, WrapErr};

const CANARY: &str = include_str!("fixtures/quality/feature_warnings/canary.jsonl");
const CANARY_STDERR: &str = include_str!("fixtures/quality/feature_warnings/canary.stderr");

/// Serializes tests that set the process-wide `CORDIAL_CARGO` env var.
static CARGO_ENV_LOCK: Mutex<()> = Mutex::new(());

fn canary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/quality/feature_warnings/canary.jsonl")
}

fn canary_stderr_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/quality/feature_warnings/canary.stderr")
}

fn combo(features: &[&str]) -> FeatureSet {
    features
        .iter()
        .map(|feature| (*feature).to_string())
        .collect::<BTreeSet<_>>()
}

#[test]
fn gate_is_any_over_features_only_present_when_silent() {
    cordial::init_tracing();
    let triggering = [combo(&[]), combo(&["panics"])];
    let silent = [combo(&["tracing"]), combo(&["panics", "tracing"])];
    let gate = suggest_gate(&triggering, &silent);
    assert_eq!(gate, Gate::Any(vec!["tracing".to_string()]));
    assert_eq!(
        gate.cfg_expression().as_deref(),
        Some("feature = \"tracing\"")
    );
}

#[test]
fn gate_is_any_list_when_either_feature_silences() {
    cordial::init_tracing();
    let triggering = [combo(&[])];
    let silent = [combo(&["a"]), combo(&["b"])];
    let gate = suggest_gate(&triggering, &silent);
    assert_eq!(
        gate.cfg_expression().as_deref(),
        Some("any(feature = \"a\", feature = \"b\")")
    );
}

#[test]
fn gate_is_all_when_only_the_pair_silences() {
    cordial::init_tracing();
    let triggering = [combo(&[]), combo(&["a"]), combo(&["b"])];
    let silent = [combo(&["a", "b"])];
    let gate = suggest_gate(&triggering, &silent);
    assert_eq!(
        gate.cfg_expression().as_deref(),
        Some("all(feature = \"a\", feature = \"b\")")
    );
}

#[test]
fn gate_is_none_when_no_predicate_separates_the_combinations() {
    cordial::init_tracing();
    // Silent in {a} and in {b, c}, but triggering in {a, c}: `any` would
    // need `a`, which a triggering combination also has.
    let triggering = [combo(&["a", "c"])];
    let silent = [combo(&["a"]), combo(&["b", "c"])];
    assert_eq!(suggest_gate(&triggering, &silent), Gate::NoSingleGate);
    assert_eq!(Gate::NoSingleGate.cfg_expression(), None);
    assert_eq!(suggest_gate(&triggering, &[]), Gate::NoSingleGate);
}

#[test]
fn parse_attributes_messages_to_the_next_artifacts_features() {
    cordial::init_tracing();
    let run = parse_cargo_hack_output(CANARY, "canary");
    assert_eq!(run.combination_count(), 4);
    assert_eq!(run.failed_combination_count(), 1);
}

#[test]
fn records_keep_only_feature_dependent_sites() -> miette::Result<()> {
    cordial::init_tracing();
    let run = parse_cargo_hack_output(CANARY, "canary");
    let records = records_from_run(&run, Path::new("/workspace"), false, &BTreeSet::new(), 6)
        .into_diagnostic()?;
    let lines: Vec<u32> = records.iter().map(|record| record.line()).collect();
    assert_eq!(lines, vec![3, 7, 20, 30], "{records:?}");

    let import = &records[0];
    assert_eq!(import.rule_id(), FeatureWarningRuleId::Unused001);
    assert_eq!(import.lint(), "unused_imports");
    assert_eq!(import.file(), Path::new("/workspace/src/lib.rs"));
    assert!(import.message().contains("`std::fmt`"));
    assert!(import.message().contains("`std::fmt::Write`"));
    assert_eq!(import.gate(), "feature = \"tracing\"");
    assert!(import.advice().contains("#[cfg(feature = \"tracing\")]"));
    assert_eq!(
        import.triggering(),
        "2 of 4 combinations, e.g. `--no-default-features`"
    );

    let helper = &records[1];
    assert_eq!(helper.rule_id(), FeatureWarningRuleId::DeadCode002);
    assert_eq!(helper.gate(), "feature = \"tracing\"");

    let both = &records[2];
    assert_eq!(both.rule_id(), FeatureWarningRuleId::DeadCode002);
    assert_eq!(
        both.gate(),
        "all(feature = \"panics\", feature = \"tracing\")"
    );
    Ok(())
}

#[test]
fn universal_warnings_are_ordinary_and_only_reported_on_request() -> miette::Result<()> {
    cordial::init_tracing();
    let run = parse_cargo_hack_output(CANARY, "canary");
    let records = records_from_run(&run, Path::new("/workspace"), true, &BTreeSet::new(), 6)
        .into_diagnostic()?;
    assert_eq!(records.len(), 5);
    let universal = records
        .iter()
        .find(|record| record.line() == 12)
        .ok_or_else(|| miette::miette!("universal site missing"))?;
    assert_eq!(universal.gate(), "");
    assert!(universal.advice().contains("No single feature gate"));
    Ok(())
}

#[test]
fn other_packages_and_errors_are_ignored() {
    cordial::init_tracing();
    let run = parse_cargo_hack_output(CANARY, "other");
    assert_eq!(run.combination_count(), 0);
    let records = records_from_run(&run, Path::new("/workspace"), true, &BTreeSet::new(), 6)
        .unwrap_or_default();
    assert!(records.is_empty());
}

#[test]
fn session_writes_checklist_grouped_by_gate_from_injected_cargo() -> miette::Result<()> {
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
    fs::write(crate_root.join("src/lib.rs"), "//! Crate.\n").into_diagnostic()?;
    fs::write(
        crate_root.join("cordial.toml"),
        "[feature_warnings]\nenabled = true\n",
    )
    .into_diagnostic()?;

    let fake = fixture.path().join("fake-cargo");
    let script = format!(
        "#!/bin/sh\nif [ \"$2\" = \"--version\" ]; then echo cargo-hack 0.6; exit 0; fi\ncat \"{}\" >&2\ncat \"{}\"\nexit 1\n",
        canary_stderr_path().display(),
        canary_path().display()
    );
    fs::write(&fake, script).into_diagnostic()?;
    let mut perms = fs::metadata(&fake).into_diagnostic()?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake, perms).into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic()?;

    let guard = CARGO_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = std::env::var_os("CORDIAL_CARGO");
    // SAFETY: the lock above gives this test sole ownership of the variable.
    unsafe {
        std::env::set_var("CORDIAL_CARGO", &fake);
    }
    let outcome = {
        let session = SessionBuilder::new(&crate_root)
            .with_store_root(store.path())
            .register(&*FEATURE_WARNINGS_ETIQUETTE)
            .build();
        session.run(&RunAll)
    };
    match previous {
        Some(value) => unsafe { std::env::set_var("CORDIAL_CARGO", value) },
        None => unsafe { std::env::remove_var("CORDIAL_CARGO") },
    }
    drop(guard);
    let outcome = outcome.into_diagnostic().wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 4);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("feature-warnings.csv")).into_diagnostic()?;
    assert!(csv.contains("FEATURE-WARNING-001"));
    assert!(csv.contains("FEATURE-WARNING-002"));
    assert!(csv.contains("FEATURE-WARNING-003"));

    let checklist =
        fs::read_to_string(findings_dir.join("feature-warnings.checklist.md")).into_diagnostic()?;
    assert!(checklist.contains("**Open items:** 4"));
    assert!(checklist.contains("### Does not compile: `E0432` (1)"));
    assert!(checklist.contains("unresolved import `crate::missing`"));
    assert!(
        checklist
            .find("Does not compile")
            .zip(checklist.find("#[cfg(feature = \"tracing\")]"))
            .is_some_and(|(failure, gate)| failure < gate),
        "failures must lead the checklist"
    );
    assert!(checklist.contains("### `#[cfg(feature = \"tracing\")]` (2)"));
    assert!(
        checklist.contains("### `#[cfg(all(feature = \"panics\", feature = \"tracing\"))]` (1)")
    );
    assert!(checklist.contains("src/lib.rs:3"));

    let summary =
        fs::read_to_string(findings_dir.join("feature-warnings-summary.md")).into_diagnostic()?;
    assert!(summary.contains("**4** feature-dependent"));
    Ok(())
}

#[test]
fn disabled_by_default_runs_nothing() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let crate_root = fixture.path().join("canary");
    fs::create_dir_all(crate_root.join("src")).into_diagnostic()?;
    fs::write(
        crate_root.join("Cargo.toml"),
        "[package]\nname = \"canary\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(crate_root.join("src/lib.rs"), "//! Crate.\n").into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(&crate_root)
        .with_store_root(store.path())
        .register(&*FEATURE_WARNINGS_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic()?;
    assert_eq!(outcome.findings().count(), 0);
    Ok(())
}

fn hack_line(reason: &str, features: &[&str], with_warning: Option<(&str, &str, u32)>) -> String {
    let id = "path+file:///workspace/p#0.1.0";
    let value = match (reason, with_warning) {
        ("message", Some((code, text, line))) => serde_json::json!({
            "reason": "compiler-message",
            "package_id": id,
            "message": {
                "level": "warning", "message": text, "code": {"code": code},
                "spans": [{"file_name": "src/lib.rs", "line_start": line, "is_primary": true}],
            },
        }),
        _ => serde_json::json!({
            "reason": "compiler-artifact", "package_id": id, "features": features,
        }),
    };
    value.to_string()
}

#[test]
fn silence_from_a_compiled_out_item_does_not_count_as_use() -> miette::Result<()> {
    cordial::init_tracing();
    // The item is gated on `shadow` and used only by `cli`. Combinations
    // without `shadow` are silent because the item does not exist.
    // Message order matters: a warning precedes the artifact it belongs to.
    let output = [
        hack_line("artifact", &[], None),
        hack_line("artifact", &["cli"], None),
        hack_line(
            "message",
            &[],
            Some(("dead_code", "function `f` is never used", 5)),
        ),
        hack_line("artifact", &["shadow"], None),
        hack_line("artifact", &["cli", "shadow"], None),
    ]
    .join("\n");
    let run = parse_cargo_hack_output(&output, "p");
    let records =
        records_from_run(&run, Path::new("/w"), false, &BTreeSet::new(), 6).into_diagnostic()?;
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].gate(), "feature = \"cli\"");
    Ok(())
}

#[test]
fn umbrella_features_are_not_named_in_a_gate() -> miette::Result<()> {
    cordial::init_tracing();
    let output = [
        hack_line(
            "message",
            &[],
            Some(("unused_imports", "unused import: `x`", 2)),
        ),
        hack_line("artifact", &["a"], None),
        hack_line("artifact", &["a", "b", "default", "full"], None),
    ]
    .join("\n");
    let run = parse_cargo_hack_output(&output, "p");
    let ignore: BTreeSet<String> = ["default", "full"].iter().map(|f| f.to_string()).collect();
    let records = records_from_run(&run, Path::new("/w"), false, &ignore, 6).into_diagnostic()?;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].gate(), "feature = \"b\"");
    Ok(())
}

#[test]
fn wide_gates_advise_an_internal_feature() -> miette::Result<()> {
    cordial::init_tracing();
    let many: Vec<String> = (0..=7).map(|index| format!("f{index}")).collect();
    let mut lines = vec![hack_line(
        "message",
        &[],
        Some(("dead_code", "function `g` is never used", 9)),
    )];
    lines.push(hack_line("artifact", &[], None));
    for feature in &many {
        lines.push(hack_line("artifact", &[feature.as_str()], None));
    }
    let run = parse_cargo_hack_output(&lines.join("\n"), "p");
    let records =
        records_from_run(&run, Path::new("/w"), false, &BTreeSet::new(), 6).into_diagnostic()?;
    assert_eq!(records.len(), 1);
    assert!(
        records[0].advice().contains("private feature"),
        "{}",
        records[0].advice()
    );
    Ok(())
}

#[test]
fn private_feature_threshold_decides_when_a_gate_is_wide() -> miette::Result<()> {
    cordial::init_tracing();
    let mut lines = vec![hack_line(
        "message",
        &[],
        Some(("dead_code", "function `g` is never used", 9)),
    )];
    lines.push(hack_line("artifact", &[], None));
    for feature in ["a", "b", "c"] {
        lines.push(hack_line("artifact", &[feature], None));
    }
    let run = parse_cargo_hack_output(&lines.join("\n"), "p");
    let narrow =
        records_from_run(&run, Path::new("/w"), false, &BTreeSet::new(), 3).into_diagnostic()?;
    let wide =
        records_from_run(&run, Path::new("/w"), false, &BTreeSet::new(), 2).into_diagnostic()?;
    assert!(!narrow[0].wide());
    assert!(wide[0].wide());
    assert!(wide[0].advice().contains("private feature"));
    Ok(())
}

#[test]
fn a_failed_build_is_named_from_the_matching_stderr_line() -> miette::Result<()> {
    cordial::init_tracing();
    let run = cordial::parse_cargo_hack_run(CANARY, CANARY_STDERR, "canary");
    let records = records_from_run(&run, Path::new("/workspace"), false, &BTreeSet::new(), 6)
        .into_diagnostic()?;
    let failure = records
        .iter()
        .find(|record| record.rule_id() == FeatureWarningRuleId::Failure003)
        .ok_or_else(|| miette::miette!("no failure record"))?;
    assert_eq!(failure.lint(), "E0432");
    assert_eq!(failure.line(), 30);
    assert_eq!(failure.message(), "unresolved import `crate::missing`");
    assert_eq!(
        failure.triggering(),
        "fails in 1 of 5 combinations, e.g. `--no-default-features --features broken`"
    );
    assert!(
        failure.advice().contains("gated out"),
        "{}",
        failure.advice()
    );
    assert_eq!(failure.gate(), "");
    // The warning the failed build printed before dying is not a finding.
    assert!(records.iter().all(|record| record.line() != 40));
    Ok(())
}

#[test]
fn without_stderr_a_failed_build_is_reported_as_an_unknown_combination() -> miette::Result<()> {
    cordial::init_tracing();
    let run = parse_cargo_hack_output(CANARY, "canary");
    let records = records_from_run(&run, Path::new("/workspace"), false, &BTreeSet::new(), 6)
        .into_diagnostic()?;
    let failure = records
        .iter()
        .find(|record| record.rule_id() == FeatureWarningRuleId::Failure003)
        .ok_or_else(|| miette::miette!("no failure record"))?;
    assert!(failure.triggering().contains("(unknown combination)"));
    Ok(())
}

#[test]
fn warnings_buffered_before_a_failed_build_do_not_leak_into_the_next_combination()
-> miette::Result<()> {
    cordial::init_tracing();
    let output = [
        hack_line(
            "message",
            &[],
            Some(("dead_code", "function `lost` is never used", 8)),
        ),
        r#"{"reason":"build-finished","success":false}"#.to_string(),
        hack_line("artifact", &["a"], None),
        r#"{"reason":"build-finished","success":true}"#.to_string(),
    ]
    .join("\n");
    let run = parse_cargo_hack_output(&output, "p");
    let records =
        records_from_run(&run, Path::new("/w"), true, &BTreeSet::new(), 6).into_diagnostic()?;
    assert!(
        records.iter().all(|record| record.line() != 8),
        "a warning from the failed build was attributed to the next one: {records:?}"
    );
    Ok(())
}

#[test]
fn a_cargo_hack_failure_with_no_build_error_still_produces_a_finding() -> miette::Result<()> {
    cordial::init_tracing();
    let stderr = "error: failed to select a version for the requirement `x = \"^9\"`\n";
    let run = parse_cargo_hack_output("", "p").with_hack_failure(stderr);
    let records =
        records_from_run(&run, Path::new("/w"), false, &BTreeSet::new(), 6).into_diagnostic()?;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].rule_id(), FeatureWarningRuleId::Failure003);
    assert_eq!(records[0].file(), Path::new("/w/Cargo.toml"));
    assert!(records[0].message().contains("failed to select a version"));
    assert_eq!(records[0].triggering(), "the whole run");
    Ok(())
}

#[test]
fn a_build_error_explains_a_failed_run_so_no_extra_hack_finding_is_added() -> miette::Result<()> {
    cordial::init_tracing();
    let run = cordial::parse_cargo_hack_run(CANARY, CANARY_STDERR, "canary")
        .with_hack_failure("error: process didn't exit successfully");
    let records = records_from_run(&run, Path::new("/workspace"), false, &BTreeSet::new(), 6)
        .into_diagnostic()?;
    let failures = records
        .iter()
        .filter(|record| record.rule_id() == FeatureWarningRuleId::Failure003)
        .count();
    assert_eq!(failures, 1);
    Ok(())
}
