//! Cross-file error apply is refused when a file is excluded, hidden, or declared elsewhere.

use cordial::run_error_file_apply;
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/error_apply_cross_file_fixtures.rs"]
mod error_apply_cross_file_fixtures;

use error_apply_cross_file_fixtures::{distributed_fixture, fixture};

#[test]
fn excluded_member_file_blocks_its_connected_error_conversion() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    let before_library = fs::read_to_string(&library).into_diagnostic()?;
    let before_child = fs::read_to_string(&child).into_diagnostic()?;
    fs::write(
        root.path().join("cordial.toml"),
        r#"
[internal_error_chain]
field_strategy = "borrowed"
[[lints.internal_error_chain.paths]]
path = "src/child.rs"
enabled = false
"#,
    )
    .into_diagnostic()?;
    let result = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(result.changed_files(), 0, "{:?}", result.blockers());
    assert_eq!(
        fs::read_to_string(&library).into_diagnostic()?,
        before_library
    );
    assert_eq!(fs::read_to_string(&child).into_diagnostic()?, before_child);
    Ok(())
}

#[test]
fn unresolved_external_field_read_blocks_entire_graph() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    let original_library = fs::read_to_string(&library).into_diagnostic()?;
    let original_child = fs::read_to_string(&child).into_diagnostic()?;
    fs::write(&child, format!("{original_child}\nfn read(error: &super::Parent<'_>) -> &super::Cause {{ &error.source }}\n")).into_diagnostic()?;
    let blocked_child = fs::read_to_string(&child).into_diagnostic()?;
    let summary = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.changed_files(), 0, "{:?}", summary.blockers());
    assert_eq!(
        fs::read_to_string(&library).into_diagnostic()?,
        original_library
    );
    assert_eq!(fs::read_to_string(&child).into_diagnostic()?, blocked_child);
    Ok(())
}

#[test]
fn hidden_external_graph_name_blocks_entire_graph() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    let original_library = fs::read_to_string(&library).into_diagnostic()?;
    let original_child = fs::read_to_string(&child).into_diagnostic()?;
    fs::write(
        &child,
        format!("{original_child}\nmacro_rules! hidden {{ () => {{ super::Parent }}; }}\n"),
    )
    .into_diagnostic()?;
    let blocked_child = fs::read_to_string(&child).into_diagnostic()?;
    let summary = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.changed_files(), 0, "{:?}", summary.blockers());
    assert_eq!(
        fs::read_to_string(&library).into_diagnostic()?,
        original_library
    );
    assert_eq!(fs::read_to_string(&child).into_diagnostic()?, blocked_child);
    Ok(())
}

#[test]
fn same_named_external_declaration_blocks_entire_graph() -> miette::Result<()> {
    cordial::init_tracing();
    let root = fixture()?;
    let library = root.path().join("src/lib.rs");
    let child = root.path().join("src/child.rs");
    let original_library = fs::read_to_string(&library).into_diagnostic()?;
    let original_child = fs::read_to_string(&child).into_diagnostic()?;
    fs::write(&child, format!("{original_child}\nstruct Parent;\n")).into_diagnostic()?;
    let blocked_child = fs::read_to_string(&child).into_diagnostic()?;
    let summary = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.changed_files(), 0, "{:?}", summary.blockers());
    assert_eq!(
        fs::read_to_string(&library).into_diagnostic()?,
        original_library
    );
    assert_eq!(fs::read_to_string(&child).into_diagnostic()?, blocked_child);
    Ok(())
}

#[test]
fn hidden_use_blocks_all_distributed_declarations() -> miette::Result<()> {
    cordial::init_tracing();
    let root = distributed_fixture()?;
    let files = ["lib.rs", "cause.rs", "kind.rs", "build.rs"];
    let build = root.path().join("src/build.rs");
    let original_build = fs::read_to_string(&build).into_diagnostic()?;
    fs::write(
        &build,
        format!("{original_build}\nmacro_rules! hidden {{ () => {{ super::Parent }}; }}\n"),
    )
    .into_diagnostic()?;
    let before: Vec<_> = files
        .iter()
        .map(|name| fs::read_to_string(root.path().join("src").join(name)))
        .collect::<Result<_, _>>()
        .into_diagnostic()?;
    let result = run_error_file_apply(root.path(), root.path(), None, false).into_diagnostic()?;
    assert_eq!(result.changed_files(), 0, "{:?}", result.blockers());
    for (name, content) in files.into_iter().zip(before) {
        assert_eq!(
            fs::read_to_string(root.path().join("src").join(name)).into_diagnostic()?,
            content
        );
    }
    Ok(())
}
