//! Formatting evidence uses resolved error types and the function's own bounds.

use std::path::Path;

use cordial::{
    IrView, QueryBuilder, SemanticCapability, SemanticFormattingRole, SemanticFunction,
    SemanticResultRecording, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};

const TYPES: &str = include_str!("fixtures/semantic_backend/recording_types.rs.txt");
const FUNCTIONS: &str = include_str!("fixtures/semantic_backend/recording_functions.rs.txt");

#[tracing::instrument(level = "debug", skip(ir), err(level = "warn"))]
fn functions(ir: &dyn IrView, name: &str) -> Result<Vec<SemanticFunction>> {
    let query = QueryBuilder::new()
        .has_attr(cordial::ATTR_SEMANTIC_FUNCTION)
        .build();
    let mut found = Vec::new();
    for node in ir.nodes_matching(&query) {
        let facts = ir
            .semantic_function(node.id())
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing semantic payload"))?;
        if facts.label() == name {
            found.push(facts);
        }
    }
    assert!(!found.is_empty(), "missing {name}");
    Ok(found)
}

#[tracing::instrument(level = "debug", skip(ir, display, debug), err(level = "warn"))]
fn assert_capability(
    ir: &dyn IrView,
    name: &str,
    display: SemanticCapability,
    debug: SemanticCapability,
) -> Result<()> {
    for facts in functions(ir, name)? {
        assert_eq!(
            facts.result_recording(),
            &SemanticResultRecording::Result { display, debug },
            "{name}"
        );
    }
    #[cfg(feature = "tracing")]
    assert_tracing_capability(ir, name, display)?;
    Ok(())
}

#[cfg(feature = "tracing")]
#[tracing::instrument(level = "debug", skip(ir, display), err(level = "warn"))]
fn assert_tracing_capability(
    ir: &dyn IrView,
    name: &str,
    display: SemanticCapability,
) -> Result<()> {
    let mut checked = 0;
    for node in ir.nodes_matching(
        &QueryBuilder::new()
            .has_attr(cordial::ATTR_SOURCE_TRACING_INVENTORY)
            .build(),
    ) {
        let inventory = ir
            .source_tracing_inventory(node.id())
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing tracing inventory"))?;
        for function in inventory
            .functions()
            .iter()
            .filter(|function| function.path().rsplit("::").next() == Some(name))
        {
            assert!(
                function.context().returns_result(),
                "resolved Result identity for {name}"
            );
            assert_eq!(
                function.context().err_is_displayable(),
                display == SemanticCapability::Proven,
                "tracing capability for {name}"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "missing tracing declaration {name}");
    Ok(())
}

#[tracing::instrument(level = "debug", skip(root), err(level = "warn"))]
fn check_specimen(root: &Path) -> Result<()> {
    let output = std::process::Command::new("cargo")
        .args(["check", "--offline", "--quiet", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env("RUSTFLAGS", "-Dwarnings")
        .output()
        .into_diagnostic()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn recording_contracts_resolve_cross_file_bounds_and_invalidate() -> Result<()> {
    cordial::init_cli_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(root.path().join("src")).into_diagnostic()?;
    std::fs::write(root.path().join("Cargo.toml"),
        "[package]\nname = \"recording_specimen\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n").into_diagnostic()?;
    for (file, text) in [
        ("lib.rs", "pub mod types; pub mod functions;\n"),
        ("types.rs", TYPES),
        ("functions.rs", FUNCTIONS),
    ] {
        std::fs::write(root.path().join("src").join(file), text).into_diagnostic()?;
    }
    check_specimen(root.path())?;
    let paths: Vec<_> = ["lib.rs", "types.rs", "functions.rs"]
        .map(|file| root.path().join("src").join(file))
        .into();
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let graph = database
        .project(
            &cordial::CrateTarget::new("recording_specimen", root.path()),
            &paths,
        )
        .into_diagnostic()?;
    use SemanticCapability::{NotProven, Proven};
    for name in [
        "cross_file",
        "borrowed_success",
        "borrowed_error",
        "read",
        "display",
    ] {
        assert_capability(&graph, name, Proven, Proven)?;
    }
    for name in ["bounded", "where_bound", "instantiated"] {
        assert_capability(&graph, name, Proven, NotProven)?;
    }
    for name in ["unbounded", "conditional_unknown", "named_error"] {
        assert_capability(&graph, name, NotProven, NotProven)?;
    }
    assert_capability(&graph, "debug_only", NotProven, Proven)?;
    for name in ["named_result", "option"] {
        for facts in functions(&graph, name)? {
            assert_eq!(
                facts.result_recording(),
                &SemanticResultRecording::NotResult
            );
        }
    }
    let formatters = functions(&graph, "fmt")?;
    assert_eq!(formatters.len(), 3);
    assert_eq!(
        formatters
            .iter()
            .filter(|facts| facts.formatting_role() == &SemanticFormattingRole::Display)
            .count(),
        2
    );
    assert_eq!(
        formatters
            .iter()
            .filter(|facts| facts.formatting_role() == &SemanticFormattingRole::Debug)
            .count(),
        1
    );
    for facts in functions(&graph, "display")? {
        assert_eq!(
            facts.formatting_role(),
            &SemanticFormattingRole::Ordinary,
            "local Display is not core Display"
        );
    }
    let snapshot = graph.snapshot().into_diagnostic()?;
    let restored = cordial::CrateIr::from_snapshot(snapshot).into_diagnostic()?;
    assert_eq!(
        functions(&graph, "bounded")?,
        functions(&restored, "bounded")?
    );
    // Revise only the obligation, preserving the Result output and type names.
    database
        .change_file(
            &paths[2],
            FUNCTIONS.replace("E: core::fmt::Display", "E: core::fmt::Debug"),
        )
        .into_diagnostic()?;
    let revised = database
        .project(
            &cordial::CrateTarget::new("recording_specimen", root.path()),
            &paths,
        )
        .into_diagnostic()?;
    for name in ["bounded", "where_bound"] {
        assert_capability(&revised, name, NotProven, Proven)?;
        assert_eq!(
            functions(&graph, name)?[0].output(),
            functions(&revised, name)?[0].output()
        );
    }
    assert_eq!(
        functions(&graph, "cross_file")?,
        functions(&revised, "cross_file")?
    );
    // A formatting implementation can change without changing any signature.
    database
        .change_file(
            &paths[1],
            TYPES.replace(
                "impl core::fmt::Display for Failure",
                "impl core::fmt::Display for Error",
            ),
        )
        .into_diagnostic()?;
    let changed_impl = database
        .project(
            &cordial::CrateTarget::new("recording_specimen", root.path()),
            &paths,
        )
        .into_diagnostic()?;
    for name in ["cross_file", "borrowed_success", "borrowed_error", "read"] {
        assert_capability(&changed_impl, name, NotProven, Proven)?;
        assert_eq!(
            functions(&revised, name)?[0].output(),
            functions(&changed_impl, name)?[0].output()
        );
    }
    assert_capability(&changed_impl, "named_error", Proven, NotProven)?;
    assert_capability(&changed_impl, "instantiated", NotProven, NotProven)?;
    // Incomplete success typing must not erase a known error capability.
    database
        .change_file(
            &paths[2],
            "pub fn missing_success() -> Result<Missing, crate::types::Error> { todo!() }\n\
             pub fn missing_error() -> Result<(), Missing> { todo!() }\n\
             pub fn missing_return() -> Missing { todo!() }\n"
                .into(),
        )
        .into_diagnostic()?;
    let partial = database
        .project(
            &cordial::CrateTarget::new("recording_specimen", root.path()),
            &paths,
        )
        .into_diagnostic()?;
    assert_capability(&partial, "missing_success", Proven, NotProven)?;
    assert_capability(
        &partial,
        "missing_error",
        SemanticCapability::Unavailable,
        SemanticCapability::Unavailable,
    )?;
    for facts in functions(&partial, "missing_return")? {
        assert_eq!(
            facts.result_recording(),
            &SemanticResultRecording::Unavailable
        );
    }
    assert_eq!(std::fs::read_to_string(&paths[1]).into_diagnostic()?, TYPES);
    assert_eq!(
        std::fs::read_to_string(&paths[2]).into_diagnostic()?,
        FUNCTIONS,
        "revisions must be virtual"
    );
    Ok(())
}
