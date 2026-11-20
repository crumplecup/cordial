//! Small process fixtures only. This executable never launches Cordial or analyzes a project.

use cordial::profiling::{
    Capture, FinishReason, Identity, Manifest, ProcessProfile, ReportStatus, read_run,
};
use std::path::Path;
use std::process::Command;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn identity() -> Identity {
    Identity {
        role: "unit_fixture".to_owned(),
        boundary: "fixture_entry".to_owned(),
        state: Some("cold".to_owned()),
        repeat: Some(2),
        ..Identity::default()
    }
}

#[hotpath::measure]
fn setup() -> Vec<u8> {
    vec![1; 1234]
}

struct Teardown(Vec<u8>);
impl Drop for Teardown {
    #[hotpath::measure]
    fn drop(&mut self) {
        teardown(self.0.len());
    }
}

#[hotpath::measure]
fn teardown(size: usize) {
    std::hint::black_box(vec![2; size]);
}

fn workload() {
    let _value = Teardown(setup());
}

#[test]
fn fixture() -> Result {
    let Ok(mode) = std::env::var("CORDIAL_LIFECYCLE_FIXTURE") else {
        return Ok(());
    };
    let root = std::env::var_os("CORDIAL_LIFECYCLE_ROOT").ok_or("fixture root")?;
    let root = Path::new(&root);
    match mode.as_str() {
        "capture" | "error" | "watchdog" | "panic" | "crash" => {
            let mut capture = Capture::start(&root.join("early"), identity())?;
            let old = capture.manifest_path();
            // Setup is already captured, although store configuration comes later.
            let bytes = setup();
            capture.configure(
                &root.join("configured"),
                Some(&root.join("export.json")),
                identity(),
            )?;
            assert!(!old.exists());
            workload();
            drop(bytes);
            std::fs::write(
                root.join("manifest-path"),
                capture.manifest_path().to_string_lossy().as_bytes(),
            )?;
            if mode == "crash" {
                std::process::exit(19);
            }
            if mode == "panic" {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    let _capture = capture;
                    panic!("controlled unwind");
                }));
                return Ok(());
            }
            let reason = match mode.as_str() {
                "error" => FinishReason::Error,
                "watchdog" => FinishReason::Watchdog,
                _ => FinishReason::Normal,
            };
            capture.finish(reason)?;
            let bytes = std::fs::read(capture.manifest_path())?;
            capture.finish(FinishReason::Error)?;
            assert_eq!(
                bytes,
                std::fs::read(capture.manifest_path())?,
                "finish exactly once"
            );
        }
        "resumed" | "missing" | "spawn_error" => {
            if std::env::var_os(cordial::RESUMABLE_VARIABLE).is_some() {
                if mode == "missing" {
                    return Ok(());
                }
                let profile = ProcessProfile::start()?;
                workload();
                if !cordial::taken_up_again() {
                    profile.finish(FinishReason::Resume);
                    std::process::exit(cordial::MEMORY_RESUME_EXIT);
                }
                profile.finish(FinishReason::Normal);
            } else {
                let profile = ProcessProfile::start_in(root, identity())?;
                let program = if mode == "spawn_error" {
                    root.join("no-such-program")
                } else {
                    std::env::current_exe()?
                };
                let result = cordial::drive_resumable(
                    &program,
                    &["--exact".into(), "fixture".into(), "--nocapture".into()],
                    &root.join("results"),
                );
                if mode == "spawn_error" {
                    assert!(result.is_err());
                } else {
                    assert_eq!(result?, 0);
                }
                profile.finish(FinishReason::Normal);
            }
        }
        _ => return Err("unknown fixture".into()),
    }
    Ok(())
}

fn launch(mode: &str, root: &Path) -> Result {
    let status = Command::new(std::env::current_exe()?)
        .args(["--exact", "fixture", "--nocapture"])
        .env("CORDIAL_LIFECYCLE_FIXTURE", mode)
        .env("CORDIAL_LIFECYCLE_ROOT", root)
        .env_remove("CORDIAL_PROFILE_TICKET")
        .env_remove("CORDIAL_RESUMABLE")
        .env_remove("CORDIAL_PERF_PROFILE_DIR")
        .env("HOTPATH_OUTPUT_PATH", root.join("must-not-overwrite.json"))
        .env("HOTPATH_OUTPUT_FORMAT", "table")
        .status()?;
    assert_eq!(status.code(), Some(if mode == "crash" { 19 } else { 0 }));
    Ok(())
}

#[test]
fn capture_includes_setup_and_teardown_and_finishes_once() -> Result {
    for (mode, reason) in [
        ("capture", FinishReason::Normal),
        ("error", FinishReason::Error),
        ("watchdog", FinishReason::Watchdog),
        ("panic", FinishReason::Panic),
    ] {
        let root = tempfile::tempdir()?;
        launch(mode, root.path())?;
        let path = std::fs::read_to_string(root.path().join("manifest-path"))?;
        let manifest: Manifest = serde_json::from_slice(&std::fs::read(&path)?)?;
        assert_eq!(manifest.completion, Some(reason));
        assert_eq!(manifest.allocation_enabled, cfg!(feature = "hotpath-alloc"));
        assert_eq!(manifest.identity, identity());
        assert_eq!(
            manifest.report_status,
            if cfg!(feature = "hotpath") {
                ReportStatus::Complete
            } else {
                ReportStatus::Disabled
            }
        );
        assert_eq!(
            read_run(Path::new(&path).parent().ok_or("parent")?)?.len(),
            1
        );
        if cfg!(feature = "hotpath") {
            let report: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&manifest.report)?)?;
            let rows = report["functions_timing"]["data"]
                .as_array()
                .ok_or("timing rows")?;
            assert!(rows.iter().any(|row| {
                row["name"]
                    .as_str()
                    .is_some_and(|name| name.ends_with("::setup"))
            }));
            assert!(rows.iter().any(|row| {
                row["name"]
                    .as_str()
                    .is_some_and(|name| name.ends_with("::teardown"))
            }));
            assert_eq!(
                std::fs::read(&manifest.report)?,
                std::fs::read(manifest.export.ok_or("export")?)?
            );
            if !cfg!(feature = "hotpath-alloc") {
                assert_eq!(report["allocation"]["functions_status"], "disabled");
            }
        }
    }
    Ok(())
}

#[test]
fn sibling_runs_and_explicit_exports_never_overwrite() -> Result {
    let root = tempfile::tempdir()?;
    launch("capture", root.path())?;
    let first = std::fs::read_to_string(root.path().join("manifest-path"))?;
    let bytes = std::fs::read(&first)?;
    launch("capture", root.path())?;
    let second = std::fs::read_to_string(root.path().join("manifest-path"))?;
    assert_ne!(first, second);
    assert_eq!(bytes, std::fs::read(first)?);
    assert!(!root.path().join("export.json").exists());
    Ok(())
}

#[test]
fn uncatchable_exit_remains_incomplete() -> Result {
    let root = tempfile::tempdir()?;
    launch("crash", root.path())?;
    let path = std::fs::read_to_string(root.path().join("manifest-path"))?;
    let records = read_run(Path::new(&path).parent().ok_or("parent")?)?;
    assert_eq!(records[0].status, ReportStatus::Running);
    assert!(
        records[0]
            .manifest
            .as_ref()
            .ok_or("manifest")?
            .completion
            .is_none()
    );
    Ok(())
}

#[test]
fn resumed_children_missing_children_and_spawn_errors_remain_linked() -> Result {
    for mode in ["resumed", "missing", "spawn_error"] {
        let root = tempfile::tempdir()?;
        launch(mode, root.path())?;
        let run = std::fs::read_dir(root.path())?
            .filter_map(std::result::Result::ok)
            .find(|entry| entry.file_name() != "results")
            .ok_or("run directory")?
            .path();
        let records = read_run(&run)?;
        let parent = records
            .iter()
            .filter_map(|record| record.manifest.as_ref())
            .find(|manifest| manifest.parent.is_none())
            .ok_or("parent")?;
        assert_eq!(parent.children.len(), if mode == "resumed" { 2 } else { 1 });
        for child in &parent.children {
            let record = records
                .iter()
                .find(|record| record.process_id == child.id)
                .ok_or("child record")?;
            if mode == "resumed" {
                let manifest = record.manifest.as_ref().ok_or("child manifest")?;
                assert_eq!(manifest.parent.as_deref(), Some(parent.process_id.as_str()));
                assert_eq!(manifest.run_id, parent.run_id);
                assert_eq!(manifest.restart, child.restart);
                assert_eq!(manifest.pid, child.pid.ok_or("child pid")?);
                assert!(child.exited);
                assert_eq!(
                    manifest.completion,
                    Some(if child.restart == 0 {
                        FinishReason::Resume
                    } else {
                        FinishReason::Normal
                    })
                );
            } else {
                assert_eq!(record.status, ReportStatus::Missing);
            }
        }
        if mode == "spawn_error" {
            assert!(parent.children[0].error.is_some());
        }
        if mode == "resumed" && cfg!(feature = "hotpath") {
            let child = records
                .iter()
                .filter_map(|record| record.manifest.as_ref())
                .find(|manifest| manifest.parent.is_some())
                .ok_or("child")?;
            std::fs::write(&child.report, "{truncated")?;
            assert!(
                read_run(&run)?
                    .iter()
                    .any(|record| record.status == ReportStatus::Malformed)
            );
        }
    }
    Ok(())
}
