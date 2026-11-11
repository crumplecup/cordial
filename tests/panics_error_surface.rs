//! The error surface of a panic site (library, binary, test) and the checklist it asks for.

use cordial::{ErrorSurface, PANICS_ETIQUETTE, RunAll, Session, SessionBuilder};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[test]
fn error_surface_classifies_library_binary_and_tests() {
    cordial::init_tracing();
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("src/lib.rs")),
        ErrorSurface::Library
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("src/main.rs")),
        ErrorSurface::Binary
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("src/bin/cordial.rs")),
        ErrorSurface::Binary
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("examples/demo.rs")),
        ErrorSurface::Binary
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("tests/panics_error_surface.rs")),
        ErrorSurface::Test
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("src/tests/unit.rs")),
        ErrorSurface::Test
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new("benches/hot.rs")),
        ErrorSurface::Test
    );
    assert_eq!(
        ErrorSurface::from_path(std::path::Path::new(
            "/home/erik/tests/myproject/src/lib.rs"
        )),
        ErrorSurface::Library,
        "a home-directory folder named tests must not reclassify library code"
    );
    assert_eq!(
        ErrorSurface::Library.expected_stack(),
        "internal error types"
    );
    assert_eq!(ErrorSurface::Binary.expected_stack(), "miette");
    assert_eq!(ErrorSurface::Test.expected_stack(), "miette");
}

#[test]
fn panics_in_tests_ask_for_miette() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("tests")).into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    fs::write(
        fixture.path().join("tests/boom.rs"),
        include_str!("fixtures/panics/sample_21.rs"),
    )
    .into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic()?;
    let checklist =
        fs::read_to_string(store.path().join("findings/panics.checklist.md")).into_diagnostic()?;
    assert!(
        checklist.contains("Tests — surface with miette"),
        "test abort sites should name miette: {checklist}"
    );
    let csv = fs::read_to_string(store.path().join("findings/panics.csv")).into_diagnostic()?;
    assert!(
        csv.contains("test"),
        "csv should record the test surface: {csv}"
    );
    Ok(())
}

#[test]
fn panics_in_binaries_ask_for_miette() -> miette::Result<()> {
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
    fs::write(
        fixture.path().join("src/main.rs"),
        include_str!("fixtures/panics/sample_22.rs"),
    )
    .into_diagnostic()
    .wrap_err("main")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let checklist = fs::read_to_string(store.path().join("findings/panics.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("Binary — surface with miette"),
        "binary abort sites should name miette: {checklist}"
    );
    let csv = fs::read_to_string(store.path().join("findings/panics.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(
        csv.contains("binary"),
        "csv should record the binary surface: {csv}"
    );
    Ok(())
}

#[test]
fn test_expect_asks_for_miette() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("tests")).into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    fs::write(
        fixture.path().join("tests/tmp.rs"),
        include_str!("fixtures/panics/sample_23.rs"),
    )
    .into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic()?;

    let csv = fs::read_to_string(store.path().join("findings/panics.csv")).into_diagnostic()?;
    assert!(
        csv.contains("PANIC-SOURCE-EXPECT") && csv.contains("test"),
        "test expect should remain in CSV: {csv}"
    );
    let checklist =
        fs::read_to_string(store.path().join("findings/panics.checklist.md")).into_diagnostic()?;
    assert!(
        checklist.contains("Tests — surface with miette"),
        "test expect must be a miette action item: {checklist}"
    );
    Ok(())
}

#[test]
fn test_unwrap_asks_for_miette() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("tests")).into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n").into_diagnostic()?;
    fs::write(
        fixture.path().join("tests/tmp.rs"),
        include_str!("fixtures/panics/sample_24.rs"),
    )
    .into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic()?;

    let csv = fs::read_to_string(store.path().join("findings/panics.csv")).into_diagnostic()?;
    assert!(
        csv.contains("PANIC-SOURCE-UNWRAP") && csv.contains("test"),
        "test unwrap should remain in CSV: {csv}"
    );
    let checklist =
        fs::read_to_string(store.path().join("findings/panics.checklist.md")).into_diagnostic()?;
    assert!(
        checklist.contains("Tests — surface with miette"),
        "test unwrap must be a miette action item: {checklist}"
    );
    Ok(())
}

#[test]
fn cfg_test_module_in_src_asks_for_miette() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/sample_25.rs"),
    )
    .into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic()?;

    let csv = fs::read_to_string(store.path().join("findings/panics.csv")).into_diagnostic()?;
    assert!(
        csv.contains("PANIC-SOURCE-UNWRAP"),
        "cfg(test) unwrap should be scanned: {csv}"
    );
    let checklist =
        fs::read_to_string(store.path().join("findings/panics.checklist.md")).into_diagnostic()?;
    assert!(
        checklist.contains("Tests — surface with miette"),
        "src cfg(test) must be the test surface, not library: {checklist}"
    );
    assert!(
        !checklist.contains("Library — return internal error types"),
        "src cfg(test) must not ask for CordialError: {checklist}"
    );
    Ok(())
}

#[test]
fn library_writeln_expect_is_checklist() -> miette::Result<()> {
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
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/sample_26.rs"),
    )
    .into_diagnostic()
    .wrap_err("lib")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;

    let csv = fs::read_to_string(store.path().join("findings/panics.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(
        csv.contains("PANIC-SOURCE-EXPECT"),
        "writeln expect should remain in inventory CSV: {csv}"
    );
    let checklist = fs::read_to_string(store.path().join("findings/panics.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("Library — return internal error types"),
        "library writeln expect must ask for a typed wrap: {checklist}"
    );
    assert!(
        !checklist.contains("**Open items:** 0"),
        "library writeln expect must be a checklist action item: {checklist}"
    );
    Ok(())
}

#[test]
fn library_tokenstream_parse_expect_is_checklist() -> miette::Result<()> {
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
    fs::write(
        fixture.path().join("src/lib.rs"),
        include_str!("fixtures/panics/sample_06.rs"),
    )
    .into_diagnostic()
    .wrap_err("lib")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*PANICS_ETIQUETTE)
        .build();
    session.run(&RunAll).into_diagnostic().wrap_err("run")?;

    let csv = fs::read_to_string(store.path().join("findings/panics.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(
        csv.contains("PANIC-SOURCE-EXPECT"),
        "tokenstream parse expect should remain in inventory CSV: {csv}"
    );
    let checklist = fs::read_to_string(store.path().join("findings/panics.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(
        checklist.contains("Library — return internal error types"),
        "library TokenStream parse expect must ask for a typed wrap: {checklist}"
    );
    assert!(
        !checklist.contains("**Open items:** 0"),
        "library TokenStream parse expect must be a checklist action item: {checklist}"
    );
    Ok(())
}
