//! Submitting: reopened and cleared claims, the open-item deny gate, skip reasons, and informational source edits.

use miette::IntoDiagnostic;
use std::fs;

#[path = "support/cli_worklist_project.rs"]
mod cli_worklist_project;

#[path = "support/cli_worklist_reading.rs"]
mod cli_worklist_reading;

use cli_worklist_project::{fixture, run};
use cli_worklist_reading::{list, stdout};

#[test]
fn submit_reopens_an_unfixed_claim_then_reports_its_clearance() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let open = run(project.path(), home.path(), &["open"])?;
    assert!(
        open.status.success(),
        "{}",
        String::from_utf8_lossy(&open.stderr)
    );
    let id = list(&store)?
        .items()
        .iter()
        .find(|item| item.item_line().contains("panic"))
        .ok_or_else(|| miette::miette!("fixture has no panic item"))?
        .id()
        .as_str()
        .to_string();
    let claimed = run(project.path(), home.path(), &["done", &id])?;
    assert!(claimed.status.success());

    let unchanged = run(project.path(), home.path(), &["submit"])?;
    assert!(
        unchanged.status.success(),
        "{}",
        String::from_utf8_lossy(&unchanged.stderr)
    );
    assert!(
        stdout(&unchanged).contains(&format!("Reopened claims: {id}.")),
        "{}",
        stdout(&unchanged)
    );
    let second = list(&store)?;
    assert_eq!(*second.record().generation(), 2);
    assert!(
        second
            .items()
            .iter()
            .any(|item| item.id().as_str() == id && *item.reopened())
    );

    let claimed_again = run(project.path(), home.path(), &["done", &id])?;
    assert!(claimed_again.status.success());
    fs::write(
        project.path().join("src/lib.rs"),
        "pub fn boom() {}\n\npub fn quiet() {}\n",
    )
    .into_diagnostic()?;
    let fixed = run(project.path(), home.path(), &["submit"])?;
    assert!(
        fixed.status.success(),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert!(
        stdout(&fixed).contains(&format!("Cleared claims: {id}.")),
        "{}",
        stdout(&fixed)
    );
    assert!(
        !list(&store)?
            .items()
            .iter()
            .any(|item| item.id().as_str() == id)
    );
    Ok(())
}

#[test]
fn submit_deny_open_scores_and_writes_before_failing() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let open = run(project.path(), home.path(), &["open"])?;
    assert!(open.status.success());
    let denied = run(project.path(), home.path(), &["submit", "--deny-open"])?;
    assert!(!denied.status.success());
    assert!(stdout(&denied).contains("new list written (list 2)"));
    assert_eq!(*list(&store)?.record().generation(), 2);
    Ok(())
}

#[cfg(feature = "homecoming_std")]
#[test]
fn submit_all_uses_the_quality_plugin_set() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let opened = run(project.path(), home.path(), &["open"])?;
    assert!(
        opened.status.success(),
        "{}",
        String::from_utf8_lossy(&opened.stderr)
    );

    let submitted = run(
        project.path(),
        home.path(),
        &["submit", "--tier", "all", "--json"],
    )?;
    assert!(
        submitted.status.success(),
        "{}",
        String::from_utf8_lossy(&submitted.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&submitted.stdout).into_diagnostic()?;
    assert_eq!(response["command"], "submit");
    assert!(
        response["gate"]["tiers_run"]
            .as_array()
            .is_some_and(|tiers| tiers.iter().any(|tier| tier == "deep"))
    );
    assert_eq!(*list(&store)?.record().generation(), 2);
    Ok(())
}

#[test]
fn skip_reason_and_pass_count_survive_submit_until_the_stop_limit() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    fs::write(
        project.path().join("cordial.toml"),
        "[worklist]\nmax_passes = 2\n",
    )
    .into_diagnostic()?;
    let opened = run(project.path(), home.path(), &["open"])?;
    assert!(
        opened.status.success(),
        "{}",
        String::from_utf8_lossy(&opened.stderr)
    );
    let before = list(&store)?;
    let current = before
        .current()
        .ok_or_else(|| miette::miette!("no current item"))?;
    let id = current.id().as_str().to_string();
    assert_eq!(*current.pass_count(), 1);

    let skipped = run(
        project.path(),
        home.path(),
        &["skip", &id, "--reason", "waiting for API"],
    )?;
    assert!(
        skipped.status.success(),
        "{}",
        String::from_utf8_lossy(&skipped.stderr)
    );
    let repeated = run(
        project.path(),
        home.path(),
        &["skip", &id, "--reason", "duplicate"],
    )?;
    assert!(repeated.status.success());
    let deferred = list(&store)?;
    let item = deferred
        .items()
        .iter()
        .find(|item| item.id().as_str() == id)
        .ok_or_else(|| miette::miette!("skipped item missing"))?;
    assert_eq!(item.skip_reasons(), &["waiting for API"]);

    let submitted = run(project.path(), home.path(), &["submit"])?;
    assert!(
        submitted.status.success(),
        "{}",
        String::from_utf8_lossy(&submitted.stderr)
    );
    assert!(
        stdout(&submitted).contains("Needs the user:"),
        "{}",
        stdout(&submitted)
    );
    assert!(
        stdout(&submitted).contains("STOP:"),
        "{}",
        stdout(&submitted)
    );
    let after = list(&store)?;
    let carried = after
        .items()
        .iter()
        .find(|item| item.id().as_str() == id)
        .ok_or_else(|| miette::miette!("carried item missing"))?;
    assert_eq!(*carried.pass_count(), 2);
    assert_eq!(carried.skip_reasons(), &["waiting for API"]);
    assert!(carried.needs_user(*after.max_passes()));
    assert!(after.items().iter().any(|item| *item.pass_count() == 1));

    let opened_again = run(project.path(), home.path(), &["open"])?;
    assert!(opened_again.status.success());
    assert!(
        stdout(&opened_again).contains("on its 2nd list"),
        "{}",
        stdout(&opened_again)
    );
    assert!(stdout(&opened_again).contains("waiting for API"));
    assert!(stdout(&opened_again).contains("STOP:"));
    let stats = run(
        project.path(),
        home.path(),
        &["stats", "--verbose", "--all"],
    )?;
    assert!(stats.status.success());
    assert!(stdout(&stats).contains("waiting for API"));
    assert!(stdout(&stats).contains("STOP:"));
    Ok(())
}

#[test]
fn source_edits_are_informational_and_age_only_appears_in_old_open_lists() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let opened = run(project.path(), home.path(), &["open"])?;
    assert!(opened.status.success());
    let generation = *list(&store)?.record().generation();

    let path = store.worklist_path();
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).into_diagnostic()?).into_diagnostic()?;
    let built_at = json["record"]["built_at_unix_secs"]
        .as_u64()
        .ok_or_else(|| miette::miette!("missing build time"))?;
    json["record"]["built_at_unix_secs"] = (built_at - 3_600).into();
    fs::write(&path, serde_json::to_vec(&json).into_diagnostic()?).into_diagnostic()?;
    fs::write(project.path().join("src/lib.rs"), "pub fn changed() {}\n").into_diagnostic()?;

    let stats = run(project.path(), home.path(), &["stats"])?;
    assert!(stats.status.success());
    assert!(stdout(&stats).contains("List age:"));
    assert!(stdout(&stats).contains("Rust sources or Cargo manifests changed since build"));
    assert!(!stdout(&stats).contains("0 Rust sources or Cargo manifests changed since build"));
    let young = run(project.path(), home.path(), &["open"])?;
    assert!(young.status.success());
    assert!(!stdout(&young).contains("List age:"));
    assert_eq!(*list(&store)?.record().generation(), generation);

    json["record"]["built_at_unix_secs"] = (built_at - 90_000).into();
    fs::write(&path, serde_json::to_vec(&json).into_diagnostic()?).into_diagnostic()?;
    let old = run(project.path(), home.path(), &["open"])?;
    assert!(old.status.success());
    assert!(stdout(&old).contains("List age:"));
    assert_eq!(*list(&store)?.record().generation(), generation);
    Ok(())
}
