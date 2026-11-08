//! Workspace membership and usable dependency edges share Cargo configuration.
#![cfg(feature = "derives")]

use cordial::{SemanticCargoOptions, SemanticFeatures, SourceDatabase};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn optional_renamed_dependency_edges_follow_loaded_features() -> Result<()> {
    cordial::init_cli_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers = ['source-error', 'consumer']\nresolver = '2'\n",
    )
    .into_diagnostic()?;
    for (name, dependencies) in [
        ("source-error", ""),
        (
            "consumer",
            "renamed = { package = 'source-error', path = '../source-error', optional = true }",
        ),
    ] {
        let member = root.path().join(name);
        std::fs::create_dir_all(member.join("src")).into_diagnostic()?;
        std::fs::write(member.join("Cargo.toml"), format!(
            "[package]\nname = '{name}'\nversion = '0.1.0'\nedition = '2024'\n[dependencies]\n{dependencies}\n"
        )).into_diagnostic()?;
        std::fs::write(member.join("src/lib.rs"), "pub struct Error;\n").into_diagnostic()?;
    }
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let disabled = database.project_path_inclusions().into_diagnostic()?;
    assert_eq!(disabled.members().len(), 2);
    let consumer = disabled
        .members()
        .iter()
        .find(|member| member.name() == "consumer")
        .ok_or_else(|| miette::miette!("missing consumer"))?;
    assert!(
        consumer.dependencies().is_empty(),
        "disabled optional dependency is not usable"
    );
    assert!(disabled.transitive_dependents("source_error").is_empty());

    let options = SemanticCargoOptions::default().with_features(SemanticFeatures::All);
    database.reload(root.path(), &options).into_diagnostic()?;
    let enabled = database.project_path_inclusions().into_diagnostic()?;
    let consumer = enabled
        .members()
        .iter()
        .find(|member| member.name() == "consumer")
        .ok_or_else(|| miette::miette!("missing enabled consumer"))?;
    assert_eq!(consumer.dependencies(), &["source_error".to_owned()].into());
    assert_eq!(
        enabled.transitive_dependents("source_error"),
        ["consumer".to_owned()].into()
    );
    assert!(
        disabled.transitive_dependents("source_error").is_empty(),
        "reload cannot mutate emitted IR"
    );
    let roundtrip: cordial::PathInclusionFacts =
        serde_json::from_value(serde_json::to_value(&enabled).into_diagnostic()?)
            .into_diagnostic()?;
    assert_eq!(roundtrip, enabled);
    Ok(())
}
