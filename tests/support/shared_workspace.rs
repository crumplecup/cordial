//! Many fixture packages in one Cargo workspace, loaded once, checked in turn.
//!
//! A semantic `SourceDatabase::load` ingests the whole sysroot and dominates a
//! small test's cost, so independent scenarios share one load. Each scenario
//! is a plain function pair: `write` builds its package, `check` reads it back
//! through the shared database. A single `#[test]` calls `run`, which keeps
//! going after a failure and reports every failing scenario together.
//!
//! The database is shared, so a scenario that calls `change_file` must leave
//! it as it found it (every edit is virtual, so restore the original text) and
//! must be listed after every read-only scenario. Scenarios run in order; a
//! mutator that fails midway can leave later scenarios reading edited text.

use cordial::SourceDatabase;
use miette::{IntoDiagnostic, Result};
use std::any::Any;
use std::borrow::Cow;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::time::Instant;

/// One fixture package and the assertions made against its loaded form.
#[derive(Clone, derive_new::new)]
pub struct Scenario {
    /// Cargo package name and directory name inside the shared workspace.
    #[new(into)]
    package: Cow<'static, str>,
    /// Create `src/` files under the package root. The harness writes the manifest.
    write: fn(&Path) -> Result<()>,
    /// Read the package back through the shared database.
    check: fn(&Path, &mut SourceDatabase) -> Result<()>,
}

impl Scenario {
    #[tracing::instrument(level = "trace", skip(self))]
    fn package(&self) -> &str {
        &self.package
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn write(&self) -> fn(&Path) -> Result<()> {
        self.write
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn check(&self) -> fn(&Path, &mut SourceDatabase) -> Result<()> {
        self.check
    }
}

/// Load every scenario's package in one workspace and run each check.
#[tracing::instrument(level = "debug", skip(scenarios), err(level = "warn"))]
pub fn run(scenarios: &[Scenario]) -> Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    let members: Vec<String> = scenarios
        .iter()
        .map(|scenario| format!("'{}'", scenario.package()))
        .collect();
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            "[workspace]\nresolver='3'\nmembers=[{}]\n",
            members.join(",")
        ),
    )
    .into_diagnostic()?;
    for scenario in scenarios {
        let member = root.join(scenario.package());
        std::fs::create_dir_all(member.join("src")).into_diagnostic()?;
        std::fs::write(
            member.join("Cargo.toml"),
            format!(
                "[package]\nname='{}'\nversion='0.1.0'\nedition='2024'\n",
                scenario.package()
            ),
        )
        .into_diagnostic()?;
        (scenario.write())(&member)?;
    }
    let started = Instant::now();
    let mut database = SourceDatabase::load(root).into_diagnostic()?;
    tracing::info!(
        scenarios = scenarios.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "shared workspace loaded"
    );
    let mut failures = Vec::new();
    for scenario in scenarios {
        let member = root.join(scenario.package());
        let began = Instant::now();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            (scenario.check())(&member, &mut database)
        }));
        let failure = match outcome {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(format!("{error:?}")),
            Err(payload) => Some(panic_message(payload.as_ref())),
        };
        tracing::info!(
            scenario = scenario.package(),
            passed = failure.is_none(),
            elapsed_ms = began.elapsed().as_millis() as u64,
            "scenario finished"
        );
        if let Some(message) = failure {
            failures.push(format!("{}: {message}", scenario.package()));
        }
    }
    miette::ensure!(
        failures.is_empty(),
        "{} of {} scenarios failed:\n{}",
        failures.len(),
        scenarios.len(),
        failures.join("\n---\n")
    );
    Ok(())
}

/// Return every Rust file under `root/src` to its on-disk text in the database.
/// Edits are virtual and the disk copy is never written, so a scenario that
/// edited the database calls this before it removes its sources.
#[tracing::instrument(level = "debug", skip(database), err(level = "warn"))]
pub fn resync(database: &mut SourceDatabase, root: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(root.join("src"))
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
    {
        let text = std::fs::read_to_string(entry.path()).into_diagnostic()?;
        database.change_file(entry.path(), text).into_diagnostic()?;
    }
    Ok(())
}

#[tracing::instrument(level = "trace", skip(payload))]
fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(ToString::to_string))
        .unwrap_or_else(|| "panicked with a non-string payload".to_string())
}
