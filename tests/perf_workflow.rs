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
        records[index].stages().iter().any(|stage| stage.name() == name)
    };
    assert!(has(0, "phase/load_and_probe"), "the cold run wrote it");
    assert_eq!(records[0].counters().get("pipeline_ran"), Some(&1));
    assert!(!has(1, "phase/load_and_probe"), "the warm run only saw an old ledger");
    assert!(has(1, "process"));
    assert_eq!(records[1].counters().get("pipeline_ran"), Some(&0));
    Ok(())
}
