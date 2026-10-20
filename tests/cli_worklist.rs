use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use miette::{IntoDiagnostic, WrapErr};

fn run(root: &Path, home: &Path, args: &[&str]) -> miette::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(root)
        .arg("--store-home")
        .arg(home)
        .args(args)
        .env("CORDIAL_NO_PROGRESS", "1")
        .output()
        .into_diagnostic()
        .wrap_err_with(|| format!("cordial {}", args.join(" ")))
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn fixture() -> miette::Result<(tempfile::TempDir, tempfile::TempDir, cordial::StoreLayout)> {
    let project = tempfile::tempdir().into_diagnostic()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(project.path().join("src")).into_diagnostic()?;
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"worklist-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        project.path().join("src/lib.rs"),
        include_str!("fixtures/panics/cli_quality.rs"),
    )
    .into_diagnostic()?;
    let slug = cordial::project_slug_from_path(project.path());
    let store = cordial::StoreLayout::from_root(home.path().join(&slug), slug);
    Ok((project, home, store))
}

fn list(store: &cordial::StoreLayout) -> miette::Result<cordial::Worklist> {
    cordial::Worklist::read(store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("worklist missing"))
}

#[test]
fn compiled_instructions_name_real_commands() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let output = run(project.path(), home.path(), &["instructions"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout(&output), cordial::WORKLIST_INSTRUCTIONS);
    assert!(!store.worklist_path().exists());

    let commands: BTreeSet<&str> = cordial::WORKLIST_INSTRUCTIONS
        .split('`')
        .skip(1)
        .step_by(2)
        .filter_map(|quoted| quoted.strip_prefix("cordial "))
        .filter_map(|invocation| invocation.split_whitespace().next())
        .collect();
    assert!(commands.len() >= 6);
    for command in commands {
        let help = run(project.path(), home.path(), &[command, "--help"])?;
        assert!(
            help.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&help.stderr)
        );
    }
    Ok(())
}

#[test]
fn open_builds_once_and_stats_reports_hard_invalidation() -> miette::Result<()> {
    cordial::init_tracing();
    let project = tempfile::tempdir().into_diagnostic()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(project.path().join("src")).into_diagnostic()?;
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"worklist-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        project.path().join("src/lib.rs"),
        include_str!("fixtures/panics/cli_quality.rs"),
    )
    .into_diagnostic()?;

    let before = run(project.path(), home.path(), &["stats"])?;
    assert!(before.status.success());
    assert!(stdout(&before).contains("No list yet."));
    assert!(stdout(&before).contains("NEXT: cordial open"));

    let first = run(project.path(), home.path(), &["open"])?;
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(stdout(&first).contains("Item 1 of"), "{}", stdout(&first));
    let slug = cordial::project_slug_from_path(project.path());
    let store = cordial::StoreLayout::from_root(home.path().join(&slug), slug);
    let list = cordial::Worklist::read(&store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("open did not write a list"))?;
    assert_eq!(*list.record().generation(), 1);
    assert!(list.current().is_some());

    let again = run(project.path(), home.path(), &["open"])?;
    assert!(again.status.success());
    let current = list
        .current()
        .ok_or_else(|| miette::miette!("open did not activate a current item"))?;
    assert!(stdout(&again).contains(current.id().as_str()));
    assert!(!stdout(&again).contains("Building one"));
    assert_eq!(
        *cordial::Worklist::read(&store)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("list missing after second open"))?
            .record()
            .generation(),
        1
    );

    let stats = run(project.path(), home.path(), &["stats", "--verbose"])?;
    assert!(stats.status.success());
    assert!(stdout(&stats).contains("[>]"), "{}", stdout(&stats));

    fs::write(
        project.path().join("cordial.toml"),
        "[panics]\nenabled = false\n",
    )
    .into_diagnostic()?;
    let stale = run(project.path(), home.path(), &["stats"])?;
    assert!(stale.status.success());
    assert!(stdout(&stale).contains("configuration changed"));
    assert!(stdout(&stale).contains("NEXT: cordial open"));
    let rebuilt = run(project.path(), home.path(), &["open"])?;
    assert!(
        rebuilt.status.success(),
        "{}",
        String::from_utf8_lossy(&rebuilt.stderr)
    );
    assert!(stdout(&rebuilt).contains("Rebuilding; 0 checked-off item(s) will be discarded."));
    assert_eq!(
        *cordial::Worklist::read(&store)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("list missing after rebuild"))?
            .record()
            .generation(),
        2
    );
    Ok(())
}

#[test]
fn open_then_submit_accepts_unchanged_config_with_maps() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    fs::write(
        project.path().join("cordial.toml"),
        "[cfg_hygiene]\ncrate_verifier = { amenable_kani = 'kani', amenable_creusot = 'creusot', amenable_verus = 'verus' }\n",
    )
    .into_diagnostic()?;

    let opened = run(project.path(), home.path(), &["open", "--json"])?;
    assert!(
        opened.status.success(),
        "{}",
        String::from_utf8_lossy(&opened.stderr)
    );
    assert_eq!(*list(&store)?.record().generation(), 1);

    let submitted = run(
        project.path(),
        home.path(),
        &["submit", "--tier", "quick", "--json"],
    )?;
    assert!(
        submitted.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&submitted),
        String::from_utf8_lossy(&submitted.stderr)
    );
    assert_eq!(*list(&store)?.record().generation(), 2);
    Ok(())
}

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
fn peek_activates_a_fresh_list_and_next_does_not_change_it() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let before = run(project.path(), home.path(), &["peek"])?;
    assert!(before.status.success());
    assert!(stdout(&before).contains("No list yet"));
    assert!(cordial::Worklist::read(&store).into_diagnostic()?.is_none());

    let gate = run(project.path(), home.path(), &["quality"])?;
    assert!(
        gate.status.success(),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let peek = run(project.path(), home.path(), &["peek"])?;
    assert!(peek.status.success());
    assert!(stdout(&peek).contains("Item 1 of"));
    let activated = list(&store)?;
    assert!(activated.current().is_some());

    let next = run(project.path(), home.path(), &["next"])?;
    assert!(!next.status.success());
    assert!(String::from_utf8_lossy(&next.stderr).contains("cordial done"));
    assert_eq!(list(&store)?, activated);
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

#[test]
fn quality_records_last_run_counts_and_hook_costs_in_the_ledger() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let gate = run(project.path(), home.path(), &["quality"])?;
    assert!(
        gate.status.success(),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let ledger = cordial::GateLedger::read(&store).into_diagnostic()?;
    let panics = ledger
        .entries()
        .get("panics")
        .ok_or_else(|| miette::miette!("panics missing from ledger"))?;
    assert!(*panics.last_run_unix_secs() > 0);
    assert!(*panics.open_count() > 0);
    assert!(*panics.wall_time_nanos() > 0);
    assert!(*ledger.last_run_wall_nanos() > 0);
    assert!(
        ledger
            .last_run_timings()
            .iter()
            .any(|timing| timing.stage() == "phase" && timing.id() == "load_and_probe")
    );
    assert!(
        ledger
            .last_run_timings()
            .iter()
            .any(|timing| timing.stage() == "pipeline" && timing.id() == "artifact_write")
    );
    Ok(())
}

#[test]
fn timings_measure_two_passes_without_replacing_quality_outputs() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let gate = run(project.path(), home.path(), &["quality"])?;
    assert!(gate.status.success());
    let report_path = store.findings_dir().join("quality-report.md");
    let previous_report = fs::read(&report_path).into_diagnostic()?;
    let previous_ledger = cordial::GateLedger::read(&store).into_diagnostic()?;

    let measured = run(
        project.path(),
        home.path(),
        &["timings", "--etiquette", "panics"],
    )?;
    assert!(
        measured.status.success(),
        "{}",
        String::from_utf8_lossy(&measured.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(store.root().join("timings.json")).into_diagnostic()?)
            .into_diagnostic()?;
    assert_eq!(json["version"], 1);
    assert_eq!(json["entries"][0]["id"], "panics");
    assert!(
        json["entries"][0]["cold"]["wall_time_nanos"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );
    assert!(
        json["entries"][0]["warm"]["wall_time_nanos"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );
    let markdown = fs::read_to_string(store.root().join("timings.md")).into_diagnostic()?;
    assert!(markdown.contains("## Pipeline phases"));
    assert_eq!(fs::read(report_path).into_diagnostic()?, previous_report);
    assert_eq!(
        cordial::GateLedger::read(&store).into_diagnostic()?,
        previous_ledger
    );
    Ok(())
}
