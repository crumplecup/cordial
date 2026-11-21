//! Upstream report import, provenance and missing evidence on controlled fixtures.
use cordial::StoreLayout;
use cordial::perf::{
    MemoryCapture, MemoryStatus, PerfEnvironment, PerfIdentityBuilder, PerfStore, PerfSubject,
    RunRecord, RunRecordBuilder, compare, profiled_from, read_memory, render_memory,
};
use cordial::profiling::{Boundary, FinishReason, Identity, Manifest, ReportStatus, RunReport};
use std::path::Path;
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn manifest(root: &Path) -> Manifest {
    Manifest {
        schema: 1,
        run_id: "run".into(),
        process_id: "process".into(),
        parent: None,
        restart: 0,
        pid: 42,
        executable: None,
        producer_revision: Some("fixture-revision".into()),
        producer_dirty: Some(false),
        identity: Identity {
            role: "test".into(),
            binary: Some("fixture".into()),
            test: Some("retained".into()),
            repeat: Some(1),
            boundary: "test_entry".into(),
            ..Identity::default()
        },
        features: vec!["hotpath-alloc".into()],
        hotpath_version: "unit-fixture".into(),
        timing_enabled: true,
        allocation_enabled: true,
        accounting: "exclusive".into(),
        traffic_metric: "bytes".into(),
        started_ns: 10,
        finished_ns: Some(20),
        completion: Some(FinishReason::Normal),
        report_status: ReportStatus::Complete,
        report: root.join("report.json"),
        export: None,
        staging: root.join("staging.json"),
        children: vec![],
        boundaries: vec![Boundary {
            region: "fixture".into(),
            evidence: "controlled numeric fixture".into(),
            unverified: String::new(),
        }],
        overrides: Default::default(),
        error: None,
    }
}

fn capture(manifest: &Manifest) -> MemoryCapture {
    MemoryCapture::read(RunReport {
        process_id: manifest.process_id.clone(),
        expected: Some(manifest.identity.clone()),
        manifest: Some(manifest.clone()),
        observation: None,
        status: ReportStatus::Complete,
    })
}

fn record(memory: Vec<MemoryCapture>) -> Result<RunRecord> {
    Ok(RunRecordBuilder::default()
        .identity(
            PerfIdentityBuilder::default()
                .sha("abcdef01")
                .dirty(false)
                .recorded_at(1u64)
                .build()?,
        )
        .environment(PerfEnvironment::capture(Path::new("/nonexistent")))
        .subject(PerfSubject::tests("fixture"))
        .memory(memory)
        .build()?)
}

fn upstream() -> serde_json::Value {
    serde_json::json!({"type":"hotpath_report", "functions_timing": {},
        "functions_alloc": {"profiling_mode":"alloc-bytes", "total_count":1, "included_count":1,
            "data":[{"name":"fixture", "calls":3, "total":"12.1 KB"}]},
        "threads":{"rss_bytes":"30.2 MB", "rss_bytes_max":"42.3 MB", "data":[]}})
}
fn publish(manifest: &Manifest, data: &serde_json::Value) -> Result {
    std::fs::write(&manifest.report, serde_json::to_vec(data)?)?;
    std::fs::write(
        manifest.report.with_file_name("process.manifest.json"),
        serde_json::to_vec(manifest)?,
    )?;
    Ok(())
}

#[test]
fn upstream_precision_and_provenance_survive_import_storage_and_display() -> Result {
    let root = tempfile::tempdir()?;
    let manifest = manifest(root.path());
    publish(&manifest, &upstream())?;
    let memory = read_memory(root.path())?;
    assert_eq!(memory[0].status, MemoryStatus::Available);
    assert!(!memory[0].attribution_complete());
    assert!(memory[0].allocation.is_none());
    assert_eq!(
        memory[0].functions.as_ref().ok_or("functions")?["data"][0]["total"],
        "12.1 KB"
    );
    let imported = profiled_from(&record(vec![])?, "fixture", root.path())?;
    let store = PerfStore::new(&StoreLayout::from_root(
        root.path().join("store"),
        "fixture",
    ));
    let loaded = PerfStore::load(&store.save(&imported)?)?;
    assert_eq!(loaded, imported);
    let display = render_memory(loaded.memory(), 10);
    for text in [
        "12.1 KB",
        "30.2 MB",
        "42.3 MB",
        "approximate",
        "Retained ownership",
        "unit-fixture",
    ] {
        assert!(display.contains(text), "{text}: {display}");
    }
    Ok(())
}

#[test]
fn legacy_fork_evidence_is_preserved_but_not_used_as_upstream_traffic() -> Result {
    let root = tempfile::tempdir()?;
    let manifest = manifest(root.path());
    let mut raw = upstream();
    raw["allocation"] =
        serde_json::json!({"schema_version":1,"heap":{"live_bytes":9007199254740993_u64}});
    publish(&manifest, &raw)?;
    let memory = capture(&manifest);
    assert_eq!(memory.status, MemoryStatus::Unsupported);
    assert_eq!(
        memory.allocation.as_ref().ok_or("legacy")?["heap"]["live_bytes"].as_u64(),
        Some(9007199254740993)
    );
    let saved = record(vec![memory])?;
    let encoded = serde_json::to_vec(&saved)?;
    assert_eq!(serde_json::from_slice::<RunRecord>(&encoded)?, saved);
    let comparison = compare(&saved, &saved, 0.0);
    assert!(!comparison.memory_complete());
    assert!(comparison.allocations().is_empty());
    Ok(())
}

#[test]
fn missing_disabled_malformed_and_truncated_are_not_zero() -> Result {
    let root = tempfile::tempdir()?;
    let mut manifest = manifest(root.path());
    publish(&manifest, &upstream())?;
    manifest.allocation_enabled = false;
    assert_eq!(capture(&manifest).status, MemoryStatus::Disabled);
    manifest.allocation_enabled = true;
    let mut raw = upstream();
    raw["functions_alloc"] = serde_json::Value::Null;
    publish(&manifest, &raw)?;
    assert_eq!(capture(&manifest).status, MemoryStatus::Unsupported);
    raw = upstream();
    raw["functions_alloc"]["profiling_mode"] = "alloc-count".into();
    publish(&manifest, &raw)?;
    assert_eq!(capture(&manifest).status, MemoryStatus::Malformed);
    raw = upstream();
    raw["functions_alloc"]["data"][0]["calls"] = "bad".into();
    publish(&manifest, &raw)?;
    assert_eq!(capture(&manifest).status, MemoryStatus::Malformed);
    std::fs::write(&manifest.report, "{")?;
    assert_eq!(capture(&manifest).status, MemoryStatus::Truncated);
    std::fs::write(&manifest.report, "not json")?;
    assert_eq!(capture(&manifest).status, MemoryStatus::Malformed);
    std::fs::remove_file(&manifest.report)?;
    assert_eq!(capture(&manifest).status, MemoryStatus::Missing);
    Ok(())
}

#[test]
fn comparisons_use_matching_traffic_and_withhold_incompatible_or_truncated_reports() -> Result {
    let root = tempfile::tempdir()?;
    let manifest = manifest(root.path());
    publish(&manifest, &upstream())?;
    let before = record(vec![capture(&manifest)])?;
    let mut raw = upstream();
    raw["functions_alloc"]["data"][0]["total"] = "6.0 KB".into();
    publish(&manifest, &raw)?;
    let after = record(vec![capture(&manifest)])?;
    assert_eq!(compare(&before, &after, 0.0).allocations().len(), 1);
    assert!(!compare(&before, &after, 0.0).memory_complete());
    let mut changed = manifest.clone();
    changed.accounting = "inclusive".into();
    assert!(
        compare(&before, &record(vec![capture(&changed)])?, 0.0)
            .allocations()
            .is_empty()
    );
    changed = manifest.clone();
    changed
        .overrides
        .insert("HOTPATH_SAMPLING_RATE".into(), "10".into());
    assert!(
        compare(&before, &record(vec![capture(&changed)])?, 0.0)
            .allocations()
            .is_empty()
    );
    raw["functions_alloc"]["total_count"] = 2.into();
    publish(&manifest, &raw)?;
    assert!(!capture(&manifest).issues.is_empty());
    assert!(
        compare(&before, &record(vec![capture(&manifest)])?, 0.0)
            .allocations()
            .is_empty()
    );
    Ok(())
}

#[cfg(feature = "hotpath-alloc")]
fn assert_fixture_traffic(capture: &MemoryCapture) -> Result {
    let rows = capture.functions.as_ref().ok_or("functions")?["data"]
        .as_array()
        .ok_or("rows")?;
    let row = rows
        .iter()
        .find(|row| {
            row["name"]
                .as_str()
                .is_some_and(|name| name.ends_with("::retained_fixture"))
        })
        .ok_or("fixture row")?;
    let bytes = cordial::perf::parse_bytes(row["total"].as_str().ok_or("total")?).ok_or("bytes")?;
    assert!(bytes.abs_diff(12345) < 100, "reported traffic: {row}");
    Ok(())
}

#[hotpath::measure]
fn retained_fixture() -> Vec<u8> {
    std::hint::black_box(vec![7; 12345])
}

#[test]
fn allocation_fixture() -> Result {
    let Some(root) = std::env::var_os("CORDIAL_MEMORY_FIXTURE") else {
        return Ok(());
    };
    if std::env::var_os("CORDIAL_PROFILE_TICKET").is_some() {
        let owner = cordial::profiling::ProcessProfile::start()?;
        let retained = retained_fixture();
        owner.finish(FinishReason::Normal);
        std::hint::black_box(retained);
        // Simulate a durably expected resume which never started. The workflow
        // importer must keep descendants even when they have no manifest/parent link.
        let ticket: serde_json::Value =
            serde_json::from_str(&std::env::var("CORDIAL_PROFILE_TICKET")?)?;
        let directory = Path::new(ticket["root"].as_str().ok_or("ticket root")?);
        let id = ticket["id"].as_str().ok_or("ticket id")?;
        let path = directory.join(format!("{id}.manifest.json"));
        let mut manifest: Manifest = serde_json::from_slice(&std::fs::read(&path)?)?;
        manifest.children.push(cordial::profiling::Child {
            id: format!("{id}-missing"),
            restart: 1,
            identity: manifest.identity.clone(),
            pid: None,
            exit_code: None,
            exited: false,
            error: None,
        });
        std::fs::write(path, serde_json::to_vec(&manifest)?)?;
        return Ok(());
    }
    let mut owner = cordial::profiling::Capture::start(
        Path::new(&root),
        Identity {
            role: "test".into(),
            binary: Some("fixture".into()),
            test: Some("retained_fixture".into()),
            boundary: "test_entry".into(),
            ..Identity::default()
        },
    )?;
    let retained = retained_fixture();
    owner.finish(FinishReason::Normal)?;
    std::hint::black_box(retained);
    Ok(())
}

#[test]
#[cfg(feature = "hotpath-alloc")]
fn controlled_allocation_survives_capture_import_storage_and_display() -> Result {
    let root = tempfile::tempdir()?;
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "allocation_fixture"])
        .env("CORDIAL_MEMORY_FIXTURE", root.path())
        .env("HOTPATH_METRICS_SERVER_OFF", "1")
        .env_remove("HOTPATH_OUTPUT_PATH")
        .env_remove("HOTPATH_OUTPUT_FORMAT")
        .env("HOTPATH_ALLOC_METRIC", "bytes")
        .env("HOTPATH_ALLOC_CUMULATIVE", "false")
        .env_remove("CORDIAL_PROFILE_TICKET")
        .env_remove("CORDIAL_PERF_PROFILE_DIR")
        .status()?;
    assert!(status.success());
    let memory = read_memory(root.path())?;
    assert_eq!(memory.len(), 1);
    assert_eq!(memory[0].status, MemoryStatus::Available);
    assert_fixture_traffic(&memory[0])?;
    assert!(!memory[0].attribution_complete());
    let saved = record(memory)?;
    let store = PerfStore::new(&StoreLayout::from_root(
        root.path().join("store"),
        "fixture",
    ));
    let loaded = PerfStore::load(&store.save(&saved)?)?;
    assert_eq!(loaded, saved);
    let display = render_memory(loaded.memory(), usize::MAX);
    assert!(display.contains("12.1 KB"));
    assert!(display.contains("Retained ownership"));
    Ok(())
}

#[test]
#[cfg(all(unix, feature = "hotpath-alloc"))]
fn workflow_imports_upstream_child_reports_without_a_driver_profile() -> Result {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir()?;
    let script = root.path().join("unit-child");
    let executable = std::env::current_exe()?
        .to_string_lossy()
        .replace('\'', "'\"'\"'");
    // The program is this unit fixture, not Cordial. It ignores workflow arguments.
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nHOTPATH_METRICS_SERVER_OFF=1 CORDIAL_MEMORY_FIXTURE=child exec '{executable}' --exact allocation_fixture\n"
        ),
    )?;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
    let project = root.path().join("perf");
    std::fs::create_dir_all(&project)?;
    let request = cordial::perf::WorkflowRequestBuilder::default()
        .name("unit-fixture")
        .program(script.clone())
        .repeat(2u32)
        .build()?;
    let records = cordial::perf::measure_workflow(&project, &root.path().join("work"), &request)?;
    assert_eq!(records.len(), 2);
    for record in &records {
        assert_eq!(
            record.memory().len(),
            4,
            "two attempts and both expected missing descendants"
        );
        assert_eq!(
            record
                .memory()
                .iter()
                .filter(|item| item.status == MemoryStatus::Missing)
                .count(),
            2
        );
        let mut repeats = Vec::new();
        for capture in record.memory() {
            if capture.status == MemoryStatus::Missing {
                assert!(capture.capture.expected.as_ref().is_some_and(
                    |identity| identity.state == record.state().map(|state| state.to_string())
                ));
                continue;
            }
            assert_eq!(capture.status, MemoryStatus::Available);
            let manifest = capture.capture.manifest.as_ref().ok_or("manifest")?;
            assert_eq!(
                manifest.producer_revision.as_deref(),
                option_env!("CORDIAL_BUILD_REVISION")
            );
            assert_eq!(
                manifest.identity.state,
                record.state().map(|state| state.to_string())
            );
            repeats.push(manifest.identity.repeat);
            assert_fixture_traffic(capture)?;
            assert!(
                capture
                    .capture
                    .observation
                    .as_ref()
                    .ok_or("observation")?
                    .exited
            );
        }
        repeats.sort();
        assert_eq!(repeats, vec![Some(1), Some(2)]);
    }
    let earlier_reports: Vec<_> = records
        .iter()
        .flat_map(|record| record.memory())
        .filter_map(|capture| {
            capture
                .capture
                .manifest
                .as_ref()
                .map(|manifest| manifest.report.clone())
        })
        .collect();
    let request = cordial::perf::WorkflowRequestBuilder::default()
        .name("unit-fixture-cold")
        .program(script)
        .repeat(1u32)
        .cold_only(true)
        .memory_limit(Some(0u64))
        .memory_target(Some(0u64))
        .build()?;
    let cold = cordial::perf::measure_workflow(&project, &root.path().join("work"), &request)?;
    assert_eq!(cold.len(), 1);
    assert_eq!(cold[0].state(), Some(cordial::perf::PerfState::Cold));
    assert_eq!(
        cold[0]
            .memory()
            .iter()
            .filter(|capture| capture.status == MemoryStatus::Available)
            .count(),
        1
    );
    assert!(
        earlier_reports.iter().all(|path| path.is_file()),
        "resetting the working cache preserves earlier raw captures"
    );
    let store = PerfStore::new(&StoreLayout::from_root(root.path().join("history"), "perf"));
    let loaded = PerfStore::load(&store.save(&cold[0])?)?;
    assert_eq!(loaded, cold[0]);
    Ok(())
}
