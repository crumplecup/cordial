//! A Cargo workspace exercising build outputs, cfg, dependencies and expansion.

use cordial::{IrView, SemanticFunction, SemanticSnapshot, SemanticType};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

#[tracing::instrument(level = "debug", err(level = "warn"))]
pub(crate) fn specimen() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    for directory in ["src", "macros/src", "dependency/src", "compiled_helper/src"] {
        std::fs::create_dir_all(root.path().join(directory)).into_diagnostic()?;
    }
    for (path, text) in [
        (
            "Cargo.toml",
            "[package]\nname='context_specimen'\nversion='0.1.0'\nedition='2024'\n[workspace]\nmembers=['macros','dependency','compiled_helper']\n[features]\nalternate=[]\n[dependencies]\ncontext_macros={path='macros'}\ncontext_dependency={path='dependency'}\n[build-dependencies]\ncompiled_helper={path='compiled_helper'}\n",
        ),
        (
            "src/lib.rs",
            include_str!("../fixtures/semantic_workspace/lib.rs.txt"),
        ),
        (
            "build.rs",
            include_str!("../fixtures/semantic_workspace/build.rs.txt"),
        ),
        ("build_support.rs", "pub fn ready()->bool{true}\n"),
        (
            "macros/Cargo.toml",
            "[package]\nname='context_macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n[dependencies]\ncompiled_helper={path='../compiled_helper'}\n",
        ),
        (
            "macros/src/lib.rs",
            include_str!("../fixtures/semantic_workspace/macros.rs.txt"),
        ),
        ("macros/shared_macro.rs", "pub fn ready()->bool{true}\n"),
        (
            "macros/included_macro.rs",
            "fn included_ready()->bool{true}\n",
        ),
        (
            "compiled_helper/Cargo.toml",
            "[package]\nname='compiled_helper'\nversion='0.1.0'\nedition='2024'\n",
        ),
        ("compiled_helper/src/lib.rs", "pub fn ready()->bool{true}\n"),
        (
            "dependency/Cargo.toml",
            "[package]\nname='context_dependency'\nversion='0.1.0'\nedition='2024'\n",
        ),
        (
            "dependency/src/lib.rs",
            "pub fn leaf()->Result<(),std::io::Error>{Ok(())}\n",
        ),
    ] {
        std::fs::write(root.path().join(path), text).into_diagnostic()?;
    }
    Ok(root)
}

#[tracing::instrument(level = "debug", skip(session, root), err(level = "warn"))]
pub(crate) fn project(
    session: &mut cordial::SourceDatabase,
    root: &Path,
) -> Result<cordial::CrateIr> {
    session
        .project(
            &cordial::CrateTarget::new("context_specimen", root),
            &[root.join("src/lib.rs")],
        )
        .into_diagnostic()
}

#[tracing::instrument(level = "debug", skip(ir), err(level = "warn"))]
pub(crate) fn function(ir: &dyn IrView, label: &str) -> Result<SemanticFunction> {
    for node in ir.nodes_matching(
        &cordial::QueryBuilder::new()
            .has_attr(cordial::ATTR_SEMANTIC_FUNCTION)
            .build(),
    ) {
        if let Some(function) = ir.semantic_function(node.id()).into_diagnostic()?
            && function.label() == label
        {
            return Ok(function);
        }
    }
    Err(miette::miette!("missing function {label}"))
}

#[tracing::instrument(level = "debug", skip(ir), err(level = "warn"))]
pub(crate) fn snapshot(ir: &dyn IrView) -> Result<SemanticSnapshot> {
    ir.semantic_snapshot()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing context"))
}

#[tracing::instrument(level = "debug", skip(contract))]
pub(crate) fn assert_error(contract: &cordial::SemanticReturnContract, expected: &str) {
    assert!(
        matches!(contract, cordial::SemanticReturnContract::Result { error: SemanticType::Adt {label,..}, ..} if label == expected),
        "{contract:?}"
    );
}
