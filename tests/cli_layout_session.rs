//! The CLI layout lint in a session and on Cordial itself.

use cordial::{CLI_LAYOUT_ETIQUETTE, RunAll, Session, SessionBuilder};
use miette::{IntoDiagnostic, WrapErr};
use std::path::Path;

#[path = "support/cli_layout_fixture.rs"]
mod cli_layout_fixture;

use cli_layout_fixture::{cli_layout_records, write_cli_crate};
#[test]
fn dogfood_cordial_follows_cli_layout() -> miette::Result<()> {
    cordial::init_tracing();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let findings = cli_layout_records(root, "cordial").wrap_err("scan cordial")?;
    assert!(
        findings.is_empty(),
        "cordial should follow CLI layout: {findings:?}"
    );
    Ok(())
}

#[test]
fn session_writes_cli_layout_artifacts() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_cli_crate(
        r#"
use clap::Parser;

#[derive(Parser)]
pub struct Cli {
    name: String,
}
"#,
        "fn main() {}\n",
    )?;
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*CLI_LAYOUT_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    assert!(
        outcome
            .findings()
            .any(|finding| finding.rule().id() == "CLI-ACT-001"),
        "session should surface CLI-ACT"
    );
    let findings_dir = store.path().join("findings");
    assert!(findings_dir.join("cli-layout.csv").is_file());
    assert!(findings_dir.join("cli-layout.checklist.md").is_file());
    assert!(findings_dir.join("cli-layout-summary.md").is_file());
    Ok(())
}
