//! Small worker fixture; no project workload or Cordial command is executed.
#![cfg(feature = "hotpath-alloc")]
use cordial::profiling::{Capture, FinishReason, Identity};
use std::sync::Arc;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn worker_origins_are_named_on_the_executing_thread() -> Result {
    let root = tempfile::tempdir()?;
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "fixture"])
        .env("CORDIAL_WORKER_FIXTURE", root.path())
        .env_remove("CORDIAL_PERF_PROFILE_DIR")
        .status()?;
    assert!(status.success());
    let captures = cordial::perf::read_captures(root.path())?;
    let manifest = captures[0].manifest.as_ref().ok_or("missing manifest")?;
    let report: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(&manifest.report)?)?;
    let functions = report["allocation"]["functions"]
        .as_array()
        .ok_or("missing functions")?;
    let row = functions
        .iter()
        .find(|row| {
            row["name"]
                .as_str()
                .is_some_and(|name| name.ends_with("Round::work_item"))
        })
        .ok_or("missing worker origin")?;
    assert_eq!(row["calls"], 3);
    assert_eq!(row["bytes"], 3 * 4097);
    assert!(functions.iter().any(|row| {
        row["name"]
            .as_str()
            .is_some_and(|name| name.ends_with("Round::item"))
    }));
    assert_eq!(manifest.boundaries.len(), 10);
    Ok(())
}

#[test]
fn fixture() -> Result {
    let Some(root) = std::env::var_os("CORDIAL_WORKER_FIXTURE") else {
        return Ok(());
    };
    let mut capture = Capture::start(std::path::Path::new(&root), Identity::default())?;
    let items: Arc<[u8]> = Arc::from([1, 2, 3]);
    let resources = vec![(); cordial::pool_size()];
    assert_eq!(
        cordial::run_items(
            &items,
            0..3,
            resources,
            |_, _| {
                drop(std::hint::black_box(vec![7u8; 4097]));
            },
            |_| false
        ),
        3
    );
    capture.finish(FinishReason::Normal)?;
    Ok(())
}
