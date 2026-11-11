//! The hierarchy checklist: peel, split and collapse items, and the session that writes the branch ranking.

use cordial::{MODULARITY_ETIQUETTE, RunAll, Session, SessionBuilder};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/modularity_padded_module.rs"]
mod modularity_padded_module;

#[path = "support/modularity_finding_field.rs"]
mod modularity_finding_field;

use modularity_finding_field::field;
use modularity_padded_module::padded_module;

#[test]
fn top_heavy_parent_is_a_peel_checklist_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/fat"))
        .into_diagnostic()
        .wrap_err("fat")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "mod fat;\n")
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(
        fixture.path().join("src/fat/mod.rs"),
        format!("mod leaf;\n{}", padded_module(160)),
    )
    .into_diagnostic()
    .wrap_err("fat")?;
    fs::write(fixture.path().join("src/fat/leaf.rs"), padded_module(8))
        .into_diagnostic()
        .wrap_err("leaf")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\nhierarchy_min_lines = 50\nmodule_size_sigma = 10\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    assert!(
        outcome.findings().any(|finding| {
            finding.rule().id() == "MODULARITY-TOP-HEAVY"
                && field(finding, "checklist").as_deref() == Some("true")
                && field(finding, "context").as_deref() == Some("fat")
        }),
        "fat should be a peel-the-parent hit"
    );
    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("peel `fat`"),
        "checklist should name the peel action: {checklist}"
    );
    Ok(())
}

#[test]
fn lopsided_sibling_is_a_split_checklist_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "mod big;\nmod small;\n")
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(fixture.path().join("src/big.rs"), padded_module(200))
        .into_diagnostic()
        .wrap_err("big")?;
    fs::write(fixture.path().join("src/small.rs"), padded_module(60))
        .into_diagnostic()
        .wrap_err("small")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\nhierarchy_min_lines = 50\nmodule_size_sigma = 10\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    assert!(
        outcome.findings().any(|finding| {
            finding.rule().id() == "MODULARITY-LOPSIDED"
                && field(finding, "checklist").as_deref() == Some("true")
                && field(finding, "context").as_deref() == Some("big")
        }),
        "big should be the dominant sibling to split"
    );
    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("split `big`"),
        "checklist should name the split-dominant action: {checklist}"
    );
    Ok(())
}

#[test]
fn unary_nest_is_a_collapse_checklist_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/error/sources"))
        .into_diagnostic()
        .wrap_err("sources")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "mod error;\n")
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(
        fixture.path().join("src/error.rs"),
        format!("mod sources;\n{}", padded_module(40)),
    )
    .into_diagnostic()
    .wrap_err("error")?;
    fs::write(
        fixture.path().join("src/error/sources/mod.rs"),
        "mod io;\nmod local;\n",
    )
    .into_diagnostic()
    .wrap_err("sources mod")?;
    fs::write(
        fixture.path().join("src/error/sources/io.rs"),
        padded_module(80),
    )
    .into_diagnostic()
    .wrap_err("io")?;
    fs::write(
        fixture.path().join("src/error/sources/local.rs"),
        padded_module(80),
    )
    .into_diagnostic()
    .wrap_err("local")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\nhierarchy_min_lines = 50\nmodule_size_sigma = 10\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    assert!(
        outcome.findings().any(|finding| {
            finding.rule().id() == "MODULARITY-COLLAPSE"
                && field(finding, "checklist").as_deref() == Some("true")
                && field(finding, "context").as_deref() == Some("error::sources")
        }),
        "error::sources should be a collapse hit"
    );
    let checklist = fs::read_to_string(store.path().join("findings/modularity.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("collapse `error::sources`"),
        "checklist should name the collapse action: {checklist}"
    );
    Ok(())
}

#[test]
fn unary_leaf_is_not_a_collapse_checklist_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/chain_layer"))
        .into_diagnostic()
        .wrap_err("chain_layer")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "mod chain_layer;\n")
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(
        fixture.path().join("src/chain_layer/mod.rs"),
        format!("mod preds;\n{}", padded_module(40)),
    )
    .into_diagnostic()
    .wrap_err("mod")?;
    fs::write(
        fixture.path().join("src/chain_layer/preds.rs"),
        padded_module(80),
    )
    .into_diagnostic()
    .wrap_err("preds")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[modularity]\nhierarchy_min_lines = 50\nmodule_size_sigma = 10\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*MODULARITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    assert!(
        !outcome
            .findings()
            .any(|finding| finding.rule().id() == "MODULARITY-COLLAPSE"),
        "a unary leaf child is a peel, not a collapse"
    );
    Ok(())
}

#[test]
fn module_hierarchy_session_writes_branch_ranking() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src/fat"))
        .into_diagnostic()
        .wrap_err("fat")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("src/thin"))
        .into_diagnostic()
        .wrap_err("thin")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "mod fat;\nmod thin;\n")
        .into_diagnostic()
        .wrap_err("lib")?;
    fs::write(
        fixture.path().join("src/fat/mod.rs"),
        format!("mod leaf;\n{}", padded_module(80)),
    )
    .into_diagnostic()
    .wrap_err("fat")?;
    fs::write(fixture.path().join("src/fat/leaf.rs"), padded_module(8))
        .into_diagnostic()
        .wrap_err("fat leaf")?;
    fs::write(
        fixture.path().join("src/thin/mod.rs"),
        format!("mod leaf;\n{}", padded_module(8)),
    )
    .into_diagnostic()
    .wrap_err("thin")?;
    fs::write(fixture.path().join("src/thin/leaf.rs"), padded_module(80))
        .into_diagnostic()
        .wrap_err("thin leaf")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*MODULARITY_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let summary = fs::read_to_string(store.path().join("findings/modularity-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("## Stream order"));
    assert!(summary.contains("## Library branches"));
    assert!(summary.contains("## Fat leaves"));
    assert!(summary.contains("## Top-heavy parents"));
    assert!(summary.contains("## Lopsided siblings"));
    assert!(summary.contains("## Unary nests"));
    let fat_pos = summary
        .find("| `fat`")
        .ok_or_else(|| miette::miette!("fat branch"))?;
    let thin_pos = summary
        .find("| `thin`")
        .ok_or_else(|| miette::miette!("thin branch"))?;
    assert!(fat_pos < thin_pos, "fat (top-heavy) should rank above thin");
    let csv = fs::read_to_string(store.path().join("findings/modularity-branches.csv"))
        .into_diagnostic()
        .wrap_err("branches csv")?;
    assert!(csv.contains("fat,"));
    assert!(csv.contains("thin,"));
    Ok(())
}
