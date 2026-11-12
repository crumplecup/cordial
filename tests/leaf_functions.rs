//! An integration-test binary reads off the library: its functions are recorded
//! by location with the definitions their calls resolved to, under the `test` cfg
//! that makes `#[test]` functions active, and the library graph gains no node.

use cordial::{CrateTarget, IrView, SourceDatabase};
use miette::{IntoDiagnostic, Result};

const LIBRARY: &str = include_str!("fixtures/leaf_functions/lib.rs.txt");
const SMOKE: &str = include_str!("fixtures/leaf_functions/smoke.rs.txt");
const CONJUNCTION: &str = include_str!("fixtures/leaf_functions/conjunction.rs.txt");
const GATED: &str = include_str!("fixtures/leaf_functions/gated.rs.txt");

#[test]
fn a_test_function_is_recorded_with_what_it_calls() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("tests")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='leafy'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ntracing='0.1'\n[features]\ndefault=['quick']\nquick=[]\nextra=[]\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    std::fs::write(root.path().join("tests/smoke.rs"), SMOKE).into_diagnostic()?;
    std::fs::write(root.path().join("tests/gated.rs"), GATED).into_diagnostic()?;
    std::fs::write(root.path().join("tests/conjunction.rs"), CONJUNCTION).into_diagnostic()?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let graph = database
        .project_target(&CrateTarget::new("leafy", root.path()))
        .into_diagnostic()?;
    let leaves = graph.leaf_functions().into_diagnostic()?;
    assert_eq!(
        leaves.len(),
        3,
        "smoke, the feature-gated test and the one behind two gates: {leaves:?}"
    );
    let paths: Vec<_> = leaves[0]
        .callees()
        .iter()
        .map(|c| c.path().as_str())
        .collect();
    assert_eq!(paths, ["leafy::helper"]);
    assert_eq!(leaves[0].called().len(), 1);
    assert!(leaves[0].called()[0].file().ends_with("src/lib.rs"));
    Ok(())
}
