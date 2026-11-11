//! The reports a session writes: the four artifacts, and the type graph combined with compliance.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
#[path = "support/error_reports.rs"]
mod error_reports;
use cordial::{INTERNAL_ERROR_CHAIN_ETIQUETTE, RunAll, Session, SessionBuilder};

const TYPE_GRAPH_FIXTURE: &str = include_str!("fixtures/error_chain/type_graph.rs");

const COMPLIANCE_FIXTURE: &str = include_str!("fixtures/error_chain/compliance.rs");

#[test]
fn session_produces_all_four_artifacts() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/error"))
        .into_diagnostic()
        .wrap_err("error dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/error/type_graph.rs"),
        TYPE_GRAPH_FIXTURE,
    )
    .into_diagnostic()
    .wrap_err("write type graph")?;
    fs::write(fixture.path().join("src/compliance.rs"), COMPLIANCE_FIXTURE)
        .into_diagnostic()
        .wrap_err("write compliance")?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "mod compliance;\npub mod error { pub mod type_graph; }",
    )
    .into_diagnostic()
    .wrap_err("write lib")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*INTERNAL_ERROR_CHAIN_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert!(outcome.findings().count() >= 5);

    let findings_dir = store.path().join("findings");
    let graph = fs::read_to_string(findings_dir.join("internal-error-type-graph.csv"))
        .into_diagnostic()
        .wrap_err("type graph csv")?;
    assert!(graph.contains("ERROR-CHAIN-INTERNAL-LEAF"));
    assert!(graph.contains("ERROR-CHAIN-FOREIGN-BRIDGE"));

    let compliance = fs::read_to_string(findings_dir.join("internal-error-compliance.csv"))
        .into_diagnostic()
        .wrap_err("compliance csv")?;
    assert!(compliance.contains("ERROR-CHAIN-COMPLIANCE-STRINGIFY-001"));
    assert!(compliance.contains("ERROR-CHAIN-COMPLIANCE-DISCARD-TYPED-001"));

    let checklist = fs::read_to_string(findings_dir.join("internal-error-chain.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("Internal error chain checklist"));
    assert!(checklist.contains("Compliance violations"));

    let summary = fs::read_to_string(findings_dir.join("internal-error-chain-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("Internal error chain summary"));
    assert!(summary.contains("compliance violations"));
    Ok(())
}

#[test]
fn report_combines_type_graph_and_compliance() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/error"))
        .into_diagnostic()
        .wrap_err("error dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        "mod error {\n    pub mod type_graph;\n}\nmod compliance;\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/error/type_graph.rs"),
        TYPE_GRAPH_FIXTURE,
    )
    .into_diagnostic()
    .wrap_err("write type graph")?;
    fs::write(fixture.path().join("src/compliance.rs"), COMPLIANCE_FIXTURE)
        .into_diagnostic()
        .wrap_err("write compliance")?;

    let report = error_reports::internal_report(fixture.path(), "fixture")?;
    assert!(!report.type_graph().nodes().is_empty());
    assert!(!report.compliance().findings().is_empty());
    Ok(())
}
