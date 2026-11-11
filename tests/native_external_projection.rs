//! A dependency of the loaded workspace projects from the same database.

use cordial::{CrateTarget, IrView, QueryBuilder, SourceDatabase};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn dependency_package_projects_from_the_workspace_database() -> Result<()> {
    cordial::init_cli_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    let app = root.path().join("app");
    let dependency = root.path().join("dependency");
    for (dir, name, deps, source) in [
        (
            &app,
            "app",
            "[dependencies]\ndependency = { path = \"../dependency\" }\n",
            "pub fn app_entry() { dependency::dependency_fn() }\n",
        ),
        (&dependency, "dependency", "", "pub fn dependency_fn() {}\n"),
    ] {
        std::fs::create_dir_all(dir.join("src")).into_diagnostic()?;
        std::fs::write(
            dir.join("Cargo.toml"),
            format!(
                "[package]\nname = '{name}'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n{deps}"
            ),
        )
        .into_diagnostic()?;
        std::fs::write(dir.join("src/lib.rs"), source).into_diagnostic()?;
    }
    let mut database = SourceDatabase::load(&app).into_diagnostic()?;
    let graph = database
        .project_target(&CrateTarget::new("dependency", &dependency))
        .into_diagnostic()?;
    let functions: Vec<_> = graph
        .nodes_matching(&QueryBuilder::new().build())
        .into_iter()
        .filter_map(|node| node.weight().name().clone())
        .collect();
    assert!(
        functions.iter().any(|name| name == "dependency_fn"),
        "{functions:?}"
    );
    assert!(!functions.iter().any(|name| name == "app_entry"));
    Ok(())
}
