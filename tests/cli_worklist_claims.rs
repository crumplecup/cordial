//! Claiming items: atomic and idempotent claims, wildcard done, and skipping the last open item.

use miette::IntoDiagnostic;

#[path = "support/cli_worklist_project.rs"]
mod cli_worklist_project;

#[path = "support/cli_worklist_reading.rs"]
mod cli_worklist_reading;

use cli_worklist_project::{fixture, run};
use cli_worklist_reading::{list, stdout};

#[test]
fn claims_are_atomic_idempotent_and_advance_only_when_current_is_acted_on() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let gate = run(project.path(), home.path(), &["quality"])?;
    assert!(
        gate.status.success(),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let initial = list(&store)?;
    assert!(initial.cursor().is_none());
    assert!(initial.items().len() >= 2);
    let first = initial.items()[0].id().as_str().to_string();
    let second = initial.items()[1].id().as_str().to_string();

    let bare = run(project.path(), home.path(), &["done"])?;
    assert!(!bare.status.success());
    assert!(stdout(&bare).contains("NEXT: cordial open"));
    assert_eq!(list(&store)?, initial);

    let bad = run(project.path(), home.path(), &["done", &first, "unknown-id"])?;
    assert!(!bad.status.success());
    assert_eq!(list(&store)?, initial);

    let named = run(project.path(), home.path(), &["done", &second])?;
    assert!(
        named.status.success(),
        "{}",
        String::from_utf8_lossy(&named.stderr)
    );
    assert!(stdout(&named).contains(&format!("Checked off {second}:")));
    assert_eq!(
        list(&store)?.current().map(|item| item.id().as_str()),
        Some(first.as_str())
    );

    let repeated = run(project.path(), home.path(), &["done", &second])?;
    assert!(repeated.status.success());
    assert!(stdout(&repeated).contains("was already checked off"));
    assert_eq!(
        list(&store)?.current().map(|item| item.id().as_str()),
        Some(first.as_str())
    );

    let cannot_skip_done = run(project.path(), home.path(), &["skip", &second])?;
    assert!(!cannot_skip_done.status.success());
    assert_eq!(*list(&store)?.items()[1].state(), cordial::ItemState::Done);

    let skip = run(project.path(), home.path(), &["skip"])?;
    assert!(skip.status.success());
    assert!(stdout(&skip).contains(&format!("Set aside {first}:")));
    assert_ne!(
        list(&store)?.current().map(|item| item.id().as_str()),
        Some(first.as_str())
    );

    let recover = run(project.path(), home.path(), &["done", &first])?;
    assert!(recover.status.success());
    assert_eq!(*list(&store)?.items()[0].state(), cordial::ItemState::Done);
    Ok(())
}

#[test]
fn done_wildcard_claims_open_items_and_preserves_skips() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let gate = run(project.path(), home.path(), &["quality"])?;
    assert!(
        gate.status.success(),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let initial = list(&store)?;
    assert!(initial.items().len() >= 2);
    let skipped_id = initial.items()[0].id().as_str();
    let skip = run(project.path(), home.path(), &["skip", skipped_id])?;
    assert!(skip.status.success());
    let before = list(&store)?;

    let mixed = run(
        project.path(),
        home.path(),
        &["done", "*", before.items()[1].id().as_str(), "--json"],
    )?;
    assert!(!mixed.status.success());
    assert_eq!(list(&store)?, before);

    let done = run(project.path(), home.path(), &["done", "*", "--json"])?;
    assert!(
        done.status.success(),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&done.stdout).into_diagnostic()?;
    let checked = response["checked_off"]
        .as_array()
        .ok_or_else(|| miette::miette!("missing checked_off array"))?;
    assert_eq!(
        checked.len(),
        before
            .items()
            .iter()
            .filter(|item| *item.state() == cordial::ItemState::Open)
            .count()
    );
    assert!(
        checked
            .iter()
            .all(|change| change["was"] == "open" && change["now"] == "done")
    );
    let after = list(&store)?;
    assert_eq!(*after.items()[0].state(), cordial::ItemState::Skipped);
    assert!(
        after.items()[1..]
            .iter()
            .all(|item| *item.state() == cordial::ItemState::Done)
    );
    assert!(after.current().is_none());

    let repeated = run(project.path(), home.path(), &["done", "*", "--json"])?;
    assert!(repeated.status.success());
    let response: serde_json::Value = serde_json::from_slice(&repeated.stdout).into_diagnostic()?;
    assert_eq!(response["checked_off"], serde_json::json!([]));
    assert_eq!(list(&store)?, after);
    Ok(())
}

#[test]
fn skipping_the_last_open_item_completes_the_list_without_checking_it_off() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let open = run(project.path(), home.path(), &["open"])?;
    assert!(
        open.status.success(),
        "{}",
        String::from_utf8_lossy(&open.stderr)
    );
    let ids: Vec<String> = list(&store)?
        .items()
        .iter()
        .map(|item| item.id().as_str().to_string())
        .collect();
    let mut args = vec!["skip"];
    args.extend(ids.iter().map(String::as_str));
    let skipped = run(project.path(), home.path(), &args)?;
    assert!(
        skipped.status.success(),
        "{}",
        String::from_utf8_lossy(&skipped.stderr)
    );
    assert!(stdout(&skipped).contains("The list is complete."));
    assert!(stdout(&skipped).contains("item(s) were skipped"));
    assert!(list(&store)?.current().is_none());
    assert_eq!(list(&store)?.open_count(), ids.len());

    let reopened = run(project.path(), home.path(), &["open"])?;
    assert!(reopened.status.success());
    assert!(stdout(&reopened).contains("NEXT: run `cordial submit`"));
    let completed = run(project.path(), home.path(), &["done", &ids[0]])?;
    assert!(completed.status.success());
    assert_eq!(*list(&store)?.items()[0].state(), cordial::ItemState::Done);
    Ok(())
}
