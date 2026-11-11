//! Multi-line gated attributes and error levels: only insufficient levels are raised.

use cordial::run_tracing_instrument_apply;
use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

#[path = "support/tracing_apply_checklist.rs"]
mod tracing_apply_checklist;

#[path = "support/tracing_apply_crate.rs"]
mod tracing_apply_crate;

use tracing_apply_crate::{checklist_for, write_apply_fixture};

#[path = "support/tracing_apply_policy.rs"]
mod tracing_apply_policy;

use tracing_apply_checklist::checklist_for_crate;
use tracing_apply_policy::{write_policy_crate, write_policy_fixture};

/// Real bug (found applying to `amenable_kani::net_model`, 2026-08-27):
/// an existing attribute that rustfmt has wrapped across multiple physical
/// lines -- `#[cfg_attr(\n  not(kani),\n  tracing::instrument(...)\n)]`,
/// the shape a long gated recipe line commonly takes -- was invisible to
/// `collect_attr_indices`'s single-line-only scan, so a mismatch never
/// replaced it: a second, corrected attribute was inserted right below the
/// untouched original, leaving the function double-instrumented.
#[test]
fn apply_rewrites_a_multi_line_gated_attribute_without_duplicating_it() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_policy_fixture(
        &["fixture_crate"],
        "[tracing]\napply_gate_crates = { fixture_crate = \"kani\" }\n",
        &checklist_for_crate("fixture_crate", &[("Store::write_payload", 6)]),
    )?;
    write_policy_crate(
        &fixture.workspace,
        "fixture_crate",
        r#"
pub struct Store {
    root: String,
}

impl Store {
    #[cfg_attr(
        not(kani),
        tracing::instrument(level = "warn", skip(self, target, payload))
    )]
    pub fn write_payload(&self, target: &str, payload: Vec<u8>) -> String {
        format!("{}/{}/{}", self.root, target, payload.len())
    }
}
"#,
        &[],
    )?;

    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()
        .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 1, "{summary:?}");

    let updated = fs::read_to_string(fixture.workspace.join("fixture_crate/src/lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(
        updated.matches("cfg_attr").count(),
        1,
        "the stale multi-line attribute must be replaced, not left behind \
         alongside a second corrected one: {updated}"
    );
    assert!(
        !updated.contains("level = \"warn\""),
        "the mismatched level must actually be rewritten: {updated}"
    );
    Ok(())
}

#[test]
fn apply_raises_only_insufficient_error_levels() -> miette::Result<()> {
    cordial::init_tracing();
    for argument in [
        "err(level = \"trace\")",
        "err(Debug, level = \"debug\")",
        "err(level = /* preserve this */ \"info\")",
    ] {
        let source = format!(
            "#[tracing::instrument(level = \"trace\", name = \"keep, )\", skip_all, fields(note = \"ret(err)\"), ret(level = \"trace\"), {argument})]\npub fn load_file() -> Result<(), std::io::Error> {{ Ok(()) }}\n"
        );
        let fixture = write_apply_fixture(&source, &checklist_for(&[("load_file", 1)]))?;
        let first =
            run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
                .into_diagnostic()?;
        assert_eq!(first.changed_functions(), 1, "{first:?}");
        let expected = source.replacen(
            argument,
            &argument
                .replace("\"trace\"", "\"warn\"")
                .replace("\"debug\"", "\"warn\"")
                .replace("\"info\"", "\"warn\""),
            1,
        );
        // Only the event value may change: the span/return levels stay trace.
        assert_eq!(
            fs::read_to_string(&fixture.src).into_diagnostic()?,
            expected
        );
        let second =
            run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
                .into_diagnostic()?;
        assert_eq!(second.changed_functions(), 0, "{second:?}");
        assert_eq!(second.unresolved(), 0, "{second:?}");
    }
    Ok(())
}

#[test]
fn apply_preserves_sufficient_or_unresolved_error_levels() -> miette::Result<()> {
    cordial::init_tracing();
    for (argument, unresolved) in [
        ("err", 0),
        ("err(Debug)", 0),
        ("err(level = \"warn\")", 0),
        ("err(Debug, level = \"error\")", 0),
        ("err(level = EVENT_LEVEL)", 1),
        ("err(level = tracing::Level::WARN)", 1),
        ("err(level = \"warn\", level = \"debug\")", 1),
    ] {
        let source = format!(
            "#[tracing::instrument(level = \"trace\", skip_all, {argument})]\npub fn load_file() -> Result<(), std::io::Error> {{ Ok(()) }}\n"
        );
        let fixture = write_apply_fixture(&source, &checklist_for(&[("load_file", 1)]))?;
        let summary =
            run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
                .into_diagnostic()?;
        assert_eq!(summary.changed_functions(), 0, "{argument}: {summary:?}");
        assert_eq!(summary.unresolved(), unresolved, "{argument}: {summary:?}");
        assert_eq!(fs::read_to_string(&fixture.src).into_diagnostic()?, source);
    }
    Ok(())
}

#[test]
fn repaired_error_is_visible_under_info_filter_and_keeps_behavior() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"#[tracing::instrument(level = "trace", name = "retained_span", skip_all, err(Debug, level = "debug"))]
pub fn load_file() -> Result<u32, std::io::Error> {
    Err(std::io::Error::other("failure_sentinel"))
}
"#;
    let fixture = write_apply_fixture(source, &checklist_for(&[("load_file", 1)]))?;
    let crate_root = fixture
        .src
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| miette::miette!("missing crate root"))?;
    fs::write(crate_root.join("Cargo.toml"), "[package]\nname = \"fixture_crate\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\ntracing = \"0.1\"\ntracing-subscriber = { version = \"0.3\", features = [\"fmt\"] }\n").into_diagnostic()?;
    fs::write(
        fixture.src.with_file_name("main.rs"),
        r#"fn main() {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO)
        .with_ansi(false).without_time().with_writer(std::io::stdout).init();
    let error = fixture_crate::load_file().expect_err("must preserve failure");
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert_eq!(error.to_string(), "failure_sentinel");
}
"#,
    )
    .into_diagnostic()?;
    let run = || -> miette::Result<String> {
        let output = std::process::Command::new("cargo")
            .args(["run", "--offline", "--quiet", "--manifest-path"])
            .arg(crate_root.join("Cargo.toml"))
            .env("RUSTFLAGS", "-Dwarnings")
            .output()
            .into_diagnostic()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).into_diagnostic()
    };
    assert!(run()?.is_empty(), "debug event is filtered before repair");
    let summary = run_tracing_instrument_apply(&fixture.workspace, &fixture.checklist, None, false)
        .into_diagnostic()?;
    assert_eq!(summary.changed_functions(), 1, "{summary:?}");
    assert_eq!(
        fs::read_to_string(&fixture.src).into_diagnostic()?,
        source.replace(
            "err(Debug, level = \"debug\")",
            "err(Debug, level = \"warn\")"
        )
    );
    let output = run()?;
    assert_eq!(output.lines().count(), 1, "one event: {output}");
    assert!(
        output.contains("WARN") && output.contains("failure_sentinel"),
        "{output}"
    );
    assert!(
        output.contains("Custom"),
        "Debug formatting is preserved: {output}"
    );
    Ok(())
}
