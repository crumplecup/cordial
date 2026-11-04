//! Workspace splice evidence survives backend changes and virtual revisions.
#![cfg(feature = "derives")]

use cordial::{
    DerivesThresholds, SourceDatabase, scan_derives_rust_source, workspace_path_inclusions,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const OWNER: &str = "pub struct Widget { value: String }\nimpl Widget { pub fn value(&self) -> &str { &self.value } }\n";
const CONSUMER: &str = r###"
#[path = "../../owner/src/shared.rs"] mod shared;
#[cfg(any())]
mod inactive {
    #[path = r#"../../owner/src/shared.rs"#] mod nested;
    mod deeper { #[path = "../../owner/src/other.rs"] mod second; }
}
#[path = 123] mod ignored;
#[path = "../../owner/src/shared.rs"]
#[path = "../../owner/src/other.rs"] mod first_wins;
macro_rules! payload { () => { #[path = "../../owner/src/macro.rs"] mod not_a_declaration; } }
fn local() { #[path = "../../owner/src/local.rs"] mod not_in_module_tree; }
#[other::path = "../../owner/src/qualified.rs"] mod qualified;
"###;

#[tracing::instrument(level = "debug", err(level = "warn"))]
fn fixture() -> Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"owner\", \"consumer\", \"facade\"]\nresolver = \"2\"\n",
    )
    .into_diagnostic()?;
    for (name, dependencies, source) in [
        ("owner", "", "mod shared;\n"),
        ("consumer", "owner = { path = \"../owner\" }", CONSUMER),
        (
            "facade",
            "renamed = { package = \"consumer\", path = \"../consumer\" }",
            "",
        ),
    ] {
        let member = root.join(name);
        std::fs::create_dir_all(member.join("src")).into_diagnostic()?;
        std::fs::write(member.join("Cargo.toml"), format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\n{dependencies}\n")).into_diagnostic()?;
        std::fs::write(member.join("src/lib.rs"), source).into_diagnostic()?;
    }
    std::fs::write(root.join("owner/src/shared.rs"), OWNER).into_diagnostic()?;
    std::fs::write(root.join("owner/src/other.rs"), "pub struct Other;\n").into_diagnostic()?;
    // Source fixtures are in scope; integration tests and non-Rust inputs are not.
    std::fs::create_dir_all(root.join("consumer/src/fixtures")).into_diagnostic()?;
    std::fs::write(
        root.join("consumer/src/fixtures/proof.rs"),
        "#[path = \"../../../owner/src/shared.rs\"] mod fixture;\n",
    )
    .into_diagnostic()?;
    std::fs::create_dir_all(root.join("consumer/tests")).into_diagnostic()?;
    std::fs::write(
        root.join("consumer/tests/integration.rs"),
        "#[path = \"../../owner/src/other.rs\"] mod test_only;\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.join("consumer/src/ignored.txt"), "not Rust").into_diagnostic()?;
    Ok(fixture)
}

#[tracing::instrument(level = "debug", skip(facts), err(level = "warn"))]
fn getter_findings(
    facts: &cordial::PathInclusionFacts,
    root: &Path,
) -> Result<Vec<cordial::DeriveSiteRecord>> {
    let owner = root.join("owner");
    scan_derives_rust_source(
        OWNER,
        &owner.join("src/shared.rs"),
        &owner.join("src"),
        &owner,
        DerivesThresholds::default(),
        facts,
    )
    .into_diagnostic()
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn workspace_splices_use_canonical_trees_and_preserve_complete_evidence() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let fixture = fixture()?;
    let root = fixture.path();
    let legacy = workspace_path_inclusions(root).into_diagnostic()?;
    let mut database = SourceDatabase::load(root).into_diagnostic()?;
    let native = database
        .project_path_inclusions(&legacy)
        .into_diagnostic()?;
    assert_eq!(
        legacy, native,
        "full source payloads, digests, members and indexes"
    );
    let source = native
        .sources()
        .iter()
        .find(|source| source.snapshot().path() == &root.join("consumer/src/lib.rs"))
        .ok_or_else(|| miette::miette!("missing consumer source"))?;
    assert_eq!(
        source.targets(),
        &[
            "../../owner/src/shared.rs",
            "../../owner/src/shared.rs",
            "../../owner/src/other.rs",
            "../../owner/src/shared.rs"
        ]
    );
    assert_eq!(native.sources().len(), 6);
    let owner = root.join("owner");
    let shared = owner.join("src/shared.rs");
    assert_eq!(native.splice_consumers(&shared, &owner), ["consumer"]);
    assert_eq!(
        native.blocking_consumer(&shared, &owner, "derive_getters"),
        Some("consumer")
    );
    assert_eq!(native.blocking_consumer(&shared, &owner, "owner"), None);
    assert_eq!(
        native.transitive_dependents("owner"),
        std::collections::HashSet::from(["consumer".into(), "facade".into()])
    );
    assert!(
        getter_findings(&native, root)?.is_empty(),
        "unsatisfiable derive remains blocked"
    );
    let roundtrip: cordial::PathInclusionFacts =
        serde_json::from_value(serde_json::to_value(&native).into_diagnostic()?)
            .into_diagnostic()?;
    assert_eq!(
        roundtrip, native,
        "typed shared workspace evidence is serializable"
    );

    let consumer = root.join("consumer/src/lib.rs");
    let fixture_source = root.join("consumer/src/fixtures/proof.rs");
    database
        .change_file(&consumer, "// no splices\n".into())
        .into_diagnostic()?;
    database
        .change_file(&fixture_source, "// no fixture splices\n".into())
        .into_diagnostic()?;
    let changed = database
        .project_path_inclusions(&native)
        .into_diagnostic()?;
    assert_eq!(changed.members(), native.members(), "Cargo context reused");
    assert!(changed.splice_consumers(&shared, &owner).is_empty());
    assert!(
        getter_findings(&changed, root)?
            .iter()
            .any(|row| row.rule_id() == cordial::DeriveRuleId::Getter001)
    );
    assert_eq!(
        std::fs::read_to_string(&consumer).into_diagnostic()?,
        CONSUMER
    );

    database
        .change_file(&consumer, CONSUMER.into())
        .into_diagnostic()?;
    database
        .change_file(
            &fixture_source,
            std::fs::read_to_string(&fixture_source).into_diagnostic()?,
        )
        .into_diagnostic()?;
    assert_eq!(
        database
            .project_path_inclusions(&native)
            .into_diagnostic()?,
        native
    );
    database
        .change_file(&consumer, "mod {".into())
        .into_diagnostic()?;
    assert!(
        database.project_path_inclusions(&native).is_err(),
        "invalid virtual syntax cannot authorize a partial graph"
    );
    database
        .change_file(&consumer, CONSUMER.into())
        .into_diagnostic()?;
    assert_eq!(
        database
            .project_path_inclusions(&native)
            .into_diagnostic()?,
        native
    );

    // A path discovered after database loading must not disappear silently.
    std::fs::write(root.join("consumer/src/late.rs"), "mod late {}\n").into_diagnostic()?;
    let wider = workspace_path_inclusions(root).into_diagnostic()?;
    assert!(database.project_path_inclusions(&wider).is_err());
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn source_and_metadata_failures_do_not_become_empty_workspace_evidence() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let fixture = fixture()?;
    std::fs::write(fixture.path().join("consumer/src/broken.rs"), "fn {").into_diagnostic()?;
    assert!(workspace_path_inclusions(fixture.path()).is_err());
    let empty = tempfile::tempdir().into_diagnostic()?;
    let synthetic = workspace_path_inclusions(empty.path()).into_diagnostic()?;
    assert_eq!(synthetic.members().len(), 1);
    assert!(synthetic.sources().is_empty());
    std::fs::write(empty.path().join("Cargo.toml"), "[package").into_diagnostic()?;
    assert!(workspace_path_inclusions(empty.path()).is_err());
    Ok(())
}
