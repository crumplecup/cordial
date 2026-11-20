//! The workflow harness: states, stage folding, and restoring the edited file.

use cordial::perf::{PerfState, WorkflowRequestBuilder, measure_workflow};
use miette::{IntoDiagnostic, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// A stand-in for cordial that writes the ledger a real run would: slow when the
/// store is empty, fast when it already holds a ledger, medium after an edit.
const FAKE: &str = r#"#!/bin/sh
while [ $# -gt 0 ]; do
  case "$1" in
    --project) project="$2"; shift 2 ;;
    --store-home) home="$2"; shift 2 ;;
    *) shift ;;
  esac
done
dir="$home/$(basename "$project")"
if grep -q "perf edit" "$project/edited.rs"; then load=3000
elif [ -f "$dir/ledger.json" ]; then load=1000
else load=9000; fi
mkdir -p "$dir"
cat > "$dir/ledger.json" <<EOF
{"last_run_wall_nanos": 1, "last_run_timings": [
 {"stage":"phase","id":"load_and_probe","elapsed_nanos":$load,"invocations":1},
 {"stage":"enrich","id":"scope","elapsed_nanos":7,"invocations":2},
 {"stage":"enrich","id":"path-index","elapsed_nanos":5,"invocations":1}]}
EOF
"#;

fn fake_cordial(dir: &Path) -> Result<std::path::PathBuf> {
    let program = dir.join("fake-cordial");
    fs::write(&program, FAKE).into_diagnostic()?;
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).into_diagnostic()?;
    Ok(program)
}

#[test]
fn workflow_records_cold_warm_and_incremental_and_restores_the_edit() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("example");
    fs::create_dir_all(&project).into_diagnostic()?;
    fs::write(project.join("edited.rs"), "fn main() {}\n").into_diagnostic()?;
    let request = WorkflowRequestBuilder::default()
        .name("example")
        .program(fake_cordial(scratch.path())?)
        .repeat(2u32)
        .edit(Some("edited.rs".into()))
        .build()
        .into_diagnostic()?;
    let records =
        measure_workflow(&project, &scratch.path().join("work"), &request).into_diagnostic()?;
    let states: Vec<_> = records.iter().map(|record| record.state()).collect();
    assert_eq!(
        states,
        vec![
            Some(PerfState::Cold),
            Some(PerfState::Warm),
            Some(PerfState::Incremental)
        ]
    );
    let load = |index: usize| {
        records[index]
            .stages()
            .iter()
            .find(|stage| stage.name() == "phase/load_and_probe")
            .map(|stage| stage.time().median())
    };
    assert_eq!(load(0), Some(9000));
    assert_eq!(load(1), Some(1000));
    assert_eq!(load(2), Some(3000));
    let counters = records[0].counters();
    assert_eq!(counters.get("hooks.enrich"), Some(&2));
    assert_eq!(counters.get("hooks.phase"), Some(&1));
    assert_eq!(counters.get("invocations"), Some(&4));
    assert_eq!(
        fs::read_to_string(project.join("edited.rs")).into_diagnostic()?,
        "fn main() {}\n"
    );
    Ok(())
}

#[test]
fn without_an_edit_file_only_cold_and_warm_are_recorded() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("example");
    fs::create_dir_all(&project).into_diagnostic()?;
    fs::write(project.join("edited.rs"), "").into_diagnostic()?;
    let request = WorkflowRequestBuilder::default()
        .name("example")
        .program(fake_cordial(scratch.path())?)
        .repeat(1u32)
        .build()
        .into_diagnostic()?;
    let records =
        measure_workflow(&project, &scratch.path().join("work"), &request).into_diagnostic()?;
    assert_eq!(records.len(), 2);
    Ok(())
}

#[test]
fn cold_only_passes_zero_budgets_and_preserves_history_in_the_reusable_store() -> Result<()> {
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("example");
    fs::create_dir_all(&project).into_diagnostic()?;
    fs::write(project.join("edited.rs"), "original\n").into_diagnostic()?;
    let program = fake_cordial(scratch.path())?;
    let fake = fs::read_to_string(&program).into_diagnostic()?;
    let checked = fake.replace(
        "while [ $# -gt 0 ]; do",
        "limit=unset; target=unset\nwhile [ $# -gt 0 ]; do",
    ).replace(
        "    *) shift ;;",
        "    --memory-limit) limit=\"$2\"; shift 2 ;;\n    --memory-target) target=\"$2\"; shift 2 ;;\n    open) shift ;;\n    *) exit 91 ;;",
    ).replace(
        "dir=\"$home/$(basename \"$project\")\"",
        "[ \"$limit\" = 0 ] && [ \"$target\" = 0 ] || exit 92\ndir=\"$home/$(basename \"$project\")\"",
    );
    fs::write(&program, checked).into_diagnostic()?;
    let layout = cordial::StoreLayout::from_root(scratch.path().join("store"), "example");
    let store = cordial::perf::PerfStore::new(&layout);
    let work = layout.perf_dir().join("work");
    fs::create_dir_all(work.join("example")).into_diagnostic()?;
    fs::write(work.join("example/ledger.json"), "stale").into_diagnostic()?;
    let history = store.raw_dir().join("earlier.hotpath.json");
    fs::create_dir_all(store.raw_dir()).into_diagnostic()?;
    fs::write(&history, "preserved").into_diagnostic()?;
    let request = WorkflowRequestBuilder::default()
        .name("example")
        .program(program)
        .repeat(1u32)
        .edit(Some("edited.rs".into()))
        .cold_only(true)
        .memory_limit(Some(0u64))
        .memory_target(Some(0u64))
        .build()
        .into_diagnostic()?;
    let records = measure_workflow(&project, &work, &request).into_diagnostic()?;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.state(), Some(PerfState::Cold));
    assert_eq!(
        record.notes().get("memory.limit_bytes").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        record
            .notes()
            .get("memory.target_bytes")
            .map(String::as_str),
        Some("0")
    );
    assert_eq!(
        record
            .stages()
            .iter()
            .find(|stage| stage.name() == "phase/load_and_probe")
            .map(|stage| stage.time().median()),
        Some(9000)
    );
    let saved = store.save(record).into_diagnostic()?;
    assert_eq!(
        cordial::perf::PerfStore::load(&saved).into_diagnostic()?,
        *record
    );
    assert_eq!(store.list().into_diagnostic()?.len(), 1);
    assert!(store.save(record).is_err(), "history is immutable");
    assert_eq!(fs::read_to_string(history).into_diagnostic()?, "preserved");
    assert_eq!(
        fs::read_to_string(project.join("edited.rs")).into_diagnostic()?,
        "original\n"
    );
    Ok(())
}

#[test]
fn cold_measurement_cli_accepts_global_zero_budgets() -> Result<()> {
    use clap::Parser;
    let cli = cordial::Cli::try_parse_from([
        "cordial",
        "perf",
        "workflow",
        "example",
        "--cold-only",
        "--repeat",
        "1",
        "--memory-limit",
        "0",
        "--memory-target",
        "0",
    ])
    .into_diagnostic()?;
    assert_eq!(cli.memory_limit, Some(cordial::MemoryLimit::Bytes(0)));
    assert_eq!(cli.memory_target, Some(cordial::MemoryLimit::Bytes(0)));
    Ok(())
}

#[test]
fn a_failing_command_is_an_error_and_still_restores_the_edit() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("example");
    fs::create_dir_all(&project).into_diagnostic()?;
    fs::write(project.join("edited.rs"), "original\n").into_diagnostic()?;
    let program = scratch.path().join("fail");
    // Succeeds until the edit is in place, then fails: the error comes mid-edit.
    let script = "#!/bin/sh\nwhile [ $# -gt 0 ]; do case \"$1\" in --project) p=\"$2\"; shift 2;; *) shift;; esac; done\n\
                  if grep -q \"perf edit\" \"$p/edited.rs\"; then exit 3; fi\n";
    fs::write(&program, script).into_diagnostic()?;
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).into_diagnostic()?;
    let request = WorkflowRequestBuilder::default()
        .name("example")
        .program(program)
        .repeat(1u32)
        .edit(Some("edited.rs".into()))
        .build()
        .into_diagnostic()?;
    assert!(measure_workflow(&project, &scratch.path().join("work"), &request).is_err());
    assert_eq!(
        fs::read_to_string(project.join("edited.rs")).into_diagnostic()?,
        "original\n"
    );
    Ok(())
}

#[test]
fn a_run_that_did_not_rewrite_the_ledger_does_not_borrow_the_last_runs_stages() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = scratch.path().join("example");
    fs::create_dir_all(&project).into_diagnostic()?;
    // Writes a ledger only when the store is empty; later runs leave it alone.
    let program = scratch.path().join("once");
    let script = "#!/bin/sh\nwhile [ $# -gt 0 ]; do case \"$1\" in --project) p=\"$2\"; shift 2;; --store-home) h=\"$2\"; shift 2;; *) shift;; esac; done\n\
        d=\"$h/$(basename \"$p\")\"; [ -f \"$d/ledger.json\" ] && exit 0\n\
        mkdir -p \"$d\"; echo '{\"last_run_timings\":[{\"stage\":\"phase\",\"id\":\"load_and_probe\",\"elapsed_nanos\":9000,\"invocations\":1}]}' > \"$d/ledger.json\"\n";
    fs::write(&program, script).into_diagnostic()?;
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).into_diagnostic()?;
    let request = WorkflowRequestBuilder::default()
        .name("example")
        .program(program)
        .repeat(1u32)
        .build()
        .into_diagnostic()?;
    let records =
        measure_workflow(&project, &scratch.path().join("work"), &request).into_diagnostic()?;
    let has = |index: usize, name: &str| {
        records[index]
            .stages()
            .iter()
            .any(|stage| stage.name() == name)
    };
    assert!(has(0, "phase/load_and_probe"), "the cold run wrote it");
    assert_eq!(records[0].counters().get("pipeline_ran"), Some(&1));
    assert!(
        !has(1, "phase/load_and_probe"),
        "the warm run only saw an old ledger"
    );
    assert!(has(1, "process"));
    assert_eq!(records[1].counters().get("pipeline_ran"), Some(&0));
    Ok(())
}
