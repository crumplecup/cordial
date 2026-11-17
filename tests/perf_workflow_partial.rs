//! A workflow whose last state fails has still measured the first two: they are returned, and the
//! failure with them, so that what took ten minutes is not lost to what failed after it.

#![cfg(unix)]

use cordial::perf::{WorkflowRequestBuilder, measure_workflow_partial};
use std::os::unix::fs::PermissionsExt;

/// A program that succeeds for `open` and fails as the memory limit does for `submit`.
fn program(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("fake-cordial");
    std::fs::write(
        &path,
        "#!/bin/sh\nfor argument in \"$@\"; do\n  if [ \"$argument\" = submit ]; then exit 75; fi\ndone\nexit 0\n",
    )
    .expect("a script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("executable");
    path
}

#[test]
fn the_states_that_finished_are_kept_when_a_later_one_fails() {
    let scratch = tempfile::tempdir().expect("a directory");
    let project = tempfile::tempdir().expect("a directory");
    let request = WorkflowRequestBuilder::default()
        .name("specimen")
        .program(program(scratch.path()))
        .repeat(1_u32)
        .build()
        .expect("a request");
    let (records, failure) =
        measure_workflow_partial(project.path(), &scratch.path().join("work"), &request);
    assert_eq!(
        records.len(),
        1,
        "the cold state finished before the warm one failed"
    );
    assert!(failure.is_some(), "and the failure is still reported");
}
