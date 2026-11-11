//! Integration-test files are measured by the modularity lints like source files,
//! except that they are not modules of the crate's tree.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    MODULARITY_ETIQUETTE, ModuleSizeInput, RunAll, Session, SessionBuilder, build_module_hierarchy,
};

const LIB_RS: &str = include_str!("fixtures/modularity/tests_tree/lib.rs");
const LONG_TEST_RS: &str = include_str!("fixtures/modularity/tests_tree/long_test_file.rs");

fn modularity_csv() -> miette::Result<String> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let root = fixture.path();
    fs::create_dir_all(root.join("src")).into_diagnostic()?;
    fs::create_dir_all(root.join("tests/fixtures")).into_diagnostic()?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.join("cordial.toml"),
        "[modularity]\nfile_inventory_min_lines = 20\n",
    )
    .into_diagnostic()?;
    fs::write(root.join("src/lib.rs"), LIB_RS).into_diagnostic()?;
    fs::write(root.join("tests/long_test_file.rs"), LONG_TEST_RS).into_diagnostic()?;
    fs::write(root.join("tests/fixtures/long_fixture.rs"), LONG_TEST_RS).into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(root)
        .with_store_root(store.path())
        .register(&*MODULARITY_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    fs::read_to_string(store.path().join("findings/modularity.csv"))
        .into_diagnostic()
        .wrap_err("modularity.csv")
}

#[test]
fn an_oversize_integration_test_file_is_measured() -> miette::Result<()> {
    cordial::init_tracing();
    let csv = modularity_csv()?;
    assert!(
        csv.lines()
            .any(|row| row.contains("MODULARITY-FILE") && row.contains("tests/long_test_file.rs")),
        "{csv}"
    );
    Ok(())
}

#[test]
fn fixture_trees_under_tests_are_not_measured() -> miette::Result<()> {
    cordial::init_tracing();
    let csv = modularity_csv()?;
    assert!(!csv.contains("long_fixture.rs"), "{csv}");
    Ok(())
}

#[test]
fn a_test_file_joins_the_size_sample_by_its_own_context() -> miette::Result<()> {
    cordial::init_tracing();
    let csv = modularity_csv()?;
    assert!(
        csv.lines()
            .any(|row| row.contains("MODULARITY-MODULE-SIZE")
                && row.contains(",tests/long_test_file,")),
        "{csv}"
    );
    Ok(())
}

#[test]
fn a_test_file_is_not_a_module_of_the_crate_tree() {
    cordial::init_tracing();
    let tree = build_module_hierarchy(&[
        ModuleSizeInput::new("<crate>".into(), "src/lib.rs".into(), 10),
        ModuleSizeInput::new(
            "tests/long_test_file".into(),
            "tests/long_test_file.rs".into(),
            40,
        ),
    ]);
    let paths: Vec<_> = tree.iter().map(|node| node.path().as_str()).collect();
    assert_eq!(paths, ["<crate>"]);
}
