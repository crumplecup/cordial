//! Exact Hotpath import, provenance and loss handling on controlled fixtures only.
use cordial::StoreLayout;
use cordial::perf::{
    MemoryCapture, MemoryStatus, PerfEnvironment, PerfIdentityBuilder, PerfStore, PerfSubject,
    ProfileUsage, RunRecord, RunRecordBuilder, compare, profiled_from, read_memory,
    render_comparison, render_memory, render_profile,
};
use cordial::profiling::{Boundary, FinishReason, Identity, Manifest, ReportStatus, RunReport};
use hotpath::json::{
    AllocationAccounting, AllocationCoverage, AllocationReport, AllocationStatus,
    ExactFunctionAllocation, ExactThreadAllocation,
};
use std::path::Path;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
const PRECISE: u64 = 9_007_199_254_740_993;

fn heap() -> hotpath::HeapSnapshot {
    hotpath::HeapSnapshot {
        live_bytes: 12345,
        peak_live_bytes: 20000,
        live_allocations: 1,
        allocated_bytes: PRECISE,
        freed_bytes: 0,
        successful_allocations: 1,
        successful_reallocations: 0,
        deallocations: 0,
        allocation_failures: 0,
        reallocation_failures: 0,
        origin_capacity: 16,
        registered_origins: 1,
        overflow_allocations: 0,
        overflow_allocated_bytes: 0,
        registry_bytes: 32,
        live_header_bytes: 16,
        origins: vec![hotpath::RetainedOrigin {
            id: 4,
            kind: hotpath::OriginKind::Function,
            name: Some("retained".into()),
            live_bytes: 12345,
            live_allocations: 1,
            peak_live_bytes: 20000,
        }],
        peak: Some(hotpath::HeapPeak {
            schema_version: 1,
            interval: hotpath::PeakInterval::AllocatorLifetimeZeroBaseline,
            tie_policy: hotpath::PeakTiePolicy::FirstAttainment,
            event_sequence: 1,
            live_bytes: 20000,
            live_allocations: 1,
            origin_updates: 1,
            origins: vec![hotpath::PeakOrigin {
                id: 4,
                live_bytes: 20000,
                live_allocations: 1,
            }],
        }),
    }
}

fn allocation() -> AllocationReport {
    AllocationReport {
        schema_version: 1,
        heap_status: AllocationStatus::Available,
        heap_baseline: Some(heap()),
        heap: Some(heap()),
        functions_status: AllocationStatus::Available,
        accounting: Some(AllocationAccounting::Exclusive),
        functions: Some(vec![ExactFunctionAllocation {
            id: 3,
            name: "retained".into(),
            calls: 1,
            bytes: Some(PRECISE),
            count: Some(7),
        }]),
        threads: Some(vec![
            ExactThreadAllocation {
                tid: 4,
                name: Some("worker".into()),
                allocated_bytes: 10,
                freed_bytes: 0,
            },
            ExactThreadAllocation {
                tid: 5,
                name: Some("worker".into()),
                allocated_bytes: 0,
                freed_bytes: 10,
            },
        ]),
        coverage: Some(AllocationCoverage {
            thread_tracking_started: true,
            thread_capacity: 16,
            registered_threads: 2,
            depth_limit: 16,
            ..AllocationCoverage::default()
        }),
    }
}

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

fn publish(manifest: &Manifest, allocation: Option<&AllocationReport>) -> Result {
    let mut value = serde_json::json!({"type":"hotpath_report", "functions_timing": {},
        "functions_alloc": {"profiling_mode": if manifest.traffic_metric == "count" {"alloc-count"} else {"alloc-bytes"},
            "data": [{"name":"rounded", "calls":1, "total":"12.1 KB"}]}});
    if let Some(allocation) = allocation {
        value["allocation"] = serde_json::to_value(allocation)?;
    }
    std::fs::write(&manifest.report, serde_json::to_vec(&value)?)?;
    std::fs::write(
        manifest.report.with_file_name("process.manifest.json"),
        serde_json::to_vec(manifest)?,
    )?;
    Ok(())
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

#[test]
fn exact_integers_and_local_identities_survive_import_storage_and_display() -> Result {
    let root = tempfile::tempdir()?;
    let manifest = manifest(root.path());
    publish(&manifest, Some(&allocation()))?;
    let memory = read_memory(root.path())?;
    assert_eq!(memory[0].status, MemoryStatus::Available);
    assert!(
        memory[0].attribution_complete(),
        "only this controlled fixture claims completeness"
    );
    assert_eq!(
        memory[0]
            .allocation
            .as_ref()
            .ok_or("allocation")?
            .functions
            .as_ref()
            .ok_or("functions")?[0]
            .bytes,
        Some(PRECISE)
    );
    let record = profiled_from(&record(vec![])?, "fixture", root.path())?;
    assert!(
        record.allocations().is_empty(),
        "exact producers remain separate, never merged by name"
    );
    let store = PerfStore::new(&StoreLayout::from_root(
        root.path().join("store"),
        "fixture",
    ));
    let loaded = PerfStore::load(&store.save(&record)?)?;
    assert_eq!(record, loaded);
    let display = render_profile(&loaded, 10);
    for expected in [
        "9007199254740993",
        "live=12345 B",
        "thread 4",
        "thread 5",
        "individual peak=20000 B",
        "simultaneous global heap peak: 20000 B",
        "at-peak origin 4",
        "OS envelope: unavailable",
        "fixture-revision",
    ] {
        assert!(display.contains(expected), "{expected}: {display}");
    }
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest.report)?)?;
    assert_eq!(
        ProfileUsage::from_report(&raw).allocations()[0].total_bytes(),
        PRECISE
    );
    Ok(())
}

#[test]
fn peak_composition_preserves_large_integers_through_import_and_storage() -> Result {
    let root = tempfile::tempdir()?;
    let manifest = manifest(root.path());
    let mut data = allocation();
    let heap = data.heap.as_mut().ok_or("heap")?;
    heap.peak_live_bytes = PRECISE as usize;
    heap.origins[0].peak_live_bytes = PRECISE as usize;
    let peak = heap.peak.as_mut().ok_or("peak")?;
    peak.live_bytes = PRECISE as usize;
    peak.origins[0].live_bytes = PRECISE as usize;
    publish(&manifest, Some(&data))?;
    let memory = read_memory(root.path())?;
    assert_eq!(memory[0].status, MemoryStatus::Available);
    assert!(memory[0].attribution_complete());
    let record = record(memory)?;
    let store = PerfStore::new(&StoreLayout::from_root(
        root.path().join("store"),
        "fixture",
    ));
    let loaded = PerfStore::load(&store.save(&record)?)?;
    assert_eq!(loaded, record);
    assert!(
        render_memory(loaded.memory(), 1)
            .contains("simultaneous global heap peak: 9007199254740993 B")
    );
    assert!(!render_memory(loaded.memory(), 0).contains("at-peak origin"));
    assert_eq!(
        loaded.memory()[0]
            .allocation
            .as_ref()
            .and_then(|allocation| allocation.heap.as_ref())
            .and_then(|heap| heap.peak.as_ref())
            .map(|peak| peak.origins.len()),
        Some(1)
    );
    Ok(())
}

#[test]
fn legacy_heap_without_peak_composition_is_readable_but_never_substitutes_ending_origins() -> Result
{
    let root = tempfile::tempdir()?;
    let manifest = manifest(root.path());
    publish(&manifest, Some(&allocation()))?;
    let current = capture(&manifest);
    let mut raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest.report)?)?;
    for section in ["heap", "heap_baseline"] {
        raw["allocation"][section]
            .as_object_mut()
            .ok_or("heap object")?
            .remove("peak");
    }
    std::fs::write(&manifest.report, serde_json::to_vec(&raw)?)?;
    let legacy = capture(&manifest);
    assert_eq!(legacy.status, MemoryStatus::Available);
    assert_eq!(
        legacy
            .allocation
            .as_ref()
            .and_then(|allocation| allocation.heap.as_ref())
            .and_then(|heap| heap.peak.as_ref()),
        None
    );
    assert!(!legacy.attribution_complete());
    let display = render_memory(std::slice::from_ref(&legacy), 10);
    assert!(display.contains("composition: unavailable; ending origins are not a substitute"));
    assert!(!display.contains("at-peak origin"));
    let comparison = compare(&record(vec![legacy])?, &record(vec![current])?, 0.0);
    assert!(!comparison.memory_complete());
    assert!(comparison.allocations().is_empty());
    Ok(())
}

#[test]
fn invalid_peak_composition_is_rejected_instead_of_reported_as_exact() -> Result {
    let root = tempfile::tempdir()?;
    let manifest = manifest(root.path());
    for variant in 0..8 {
        let mut data = allocation();
        let peak = data
            .heap
            .as_mut()
            .ok_or("heap")?
            .peak
            .as_mut()
            .ok_or("peak")?;
        match variant {
            0 => peak.origins[0].live_bytes -= 1,
            1 => peak.origins[0].live_allocations += 1,
            2 => peak.origins.push(peak.origins[0].clone()),
            3 => peak.origins[0].id = 999,
            4 => peak.event_sequence = 2,
            5 => peak.origin_updates = 2,
            6 => peak.schema_version = 99,
            _ => {
                peak.live_bytes = usize::MAX;
                peak.origins[0].live_bytes = usize::MAX;
                peak.origins.push(hotpath::PeakOrigin {
                    id: 5,
                    live_bytes: 1,
                    live_allocations: 1,
                });
                data.heap
                    .as_mut()
                    .ok_or("heap")?
                    .origins
                    .push(hotpath::RetainedOrigin {
                        id: 5,
                        kind: hotpath::OriginKind::Function,
                        name: Some("overflowing sum".into()),
                        live_bytes: 0,
                        live_allocations: 0,
                        peak_live_bytes: 1,
                    });
            }
        }
        publish(&manifest, Some(&data))?;
        let result = capture(&manifest);
        assert_eq!(
            result.status,
            if variant == 6 {
                MemoryStatus::Unsupported
            } else {
                MemoryStatus::Malformed
            },
            "variant {variant}"
        );
        assert!(!result.attribution_complete());
        let display = render_memory(std::slice::from_ref(&result), 10);
        assert!(
            !display.contains("at-peak origin"),
            "variant {variant}: {display}"
        );
        assert!(display.contains("composition: invalid or unsupported"));
        let comparison = compare(&record(vec![result.clone()])?, &record(vec![result])?, 0.0);
        assert!(comparison.allocations().is_empty());
    }
    Ok(())
}

#[test]
fn older_manifests_leave_producer_revision_explicitly_unknown() -> Result {
    let root = tempfile::tempdir()?;
    let original = manifest(root.path());
    let mut old = serde_json::to_value(original)?;
    let fields = old.as_object_mut().ok_or("manifest object")?;
    fields.remove("producer_revision");
    fields.remove("producer_dirty");
    let restored: Manifest = serde_json::from_value(old)?;
    assert_eq!(restored.producer_revision, None);
    assert_eq!(restored.producer_dirty, None);
    Ok(())
}

#[test]
fn absent_disabled_unsupported_invalid_and_truncated_are_not_zero() -> Result {
    let root = tempfile::tempdir()?;
    let mut manifest = manifest(root.path());
    publish(&manifest, None)?;
    assert_eq!(capture(&manifest).status, MemoryStatus::Unsupported);
    manifest.allocation_enabled = false;
    assert_eq!(capture(&manifest).status, MemoryStatus::Disabled);
    manifest.accounting = "disabled".into();
    publish(&manifest, Some(&AllocationReport::disabled()))?;
    assert_eq!(capture(&manifest).status, MemoryStatus::Disabled);
    manifest = self::manifest(root.path());
    let mut data = allocation();
    data.schema_version = 99;
    publish(&manifest, Some(&data))?;
    assert_eq!(capture(&manifest).status, MemoryStatus::Unsupported);
    data = allocation();
    data.functions = None;
    publish(&manifest, Some(&data))?;
    assert_eq!(capture(&manifest).status, MemoryStatus::Malformed);
    std::fs::write(&manifest.report, "{\"allocation\":")?;
    assert_eq!(read_memory(root.path())?[0].status, MemoryStatus::Truncated);
    std::fs::write(&manifest.report, "{bad}")?;
    assert_eq!(read_memory(root.path())?[0].status, MemoryStatus::Malformed);
    std::fs::remove_file(&manifest.report)?;
    assert_eq!(read_memory(root.path())?[0].status, MemoryStatus::Missing);
    manifest.report_status = ReportStatus::Running;
    std::fs::write(
        root.path().join("process.manifest.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    assert_eq!(
        read_memory(root.path())?[0].status,
        MemoryStatus::Incomplete
    );
    Ok(())
}

#[test]
fn zero_is_available_and_count_reports_never_become_byte_reports() -> Result {
    let root = tempfile::tempdir()?;
    let mut manifest = manifest(root.path());
    let mut data = allocation();
    data.functions.as_mut().ok_or("functions")?[0].bytes = Some(0);
    publish(&manifest, Some(&data))?;
    assert_eq!(capture(&manifest).status, MemoryStatus::Available);
    assert_eq!(
        capture(&manifest)
            .allocation
            .ok_or("allocation")?
            .functions
            .ok_or("rows")?[0]
            .bytes,
        Some(0)
    );
    manifest.traffic_metric = "count".into();
    publish(&manifest, None)?;
    let raw = serde_json::from_slice(&std::fs::read(&manifest.report)?)?;
    assert!(ProfileUsage::from_report(&raw).allocations().is_empty());
    publish(&manifest, Some(&data))?;
    assert_eq!(capture(&manifest).status, MemoryStatus::Available);
    manifest.traffic_metric = "bytes".into();
    assert_eq!(
        capture(&manifest).status,
        MemoryStatus::Malformed,
        "producer/display mismatch"
    );
    Ok(())
}

#[test]
fn losses_missing_children_and_bad_inventories_preclude_complete_results() -> Result {
    let root = tempfile::tempdir()?;
    let mut manifest = manifest(root.path());
    let mut data = allocation();
    data.coverage.as_mut().ok_or("coverage")?.overflow_threads = 1;
    data.heap.as_mut().ok_or("heap")?.overflow_allocations = 1;
    manifest.children.push(cordial::profiling::Child {
        id: "absent".into(),
        restart: 1,
        identity: manifest.identity.clone(),
        pid: Some(77),
        exit_code: Some(9),
        exited: true,
        error: None,
    });
    publish(&manifest, Some(&data))?;
    std::fs::write(root.path().join("broken.manifest.json"), "{")?;
    std::fs::write(root.path().join("bad.expected.json"), "{bad}")?;
    std::fs::write(root.path().join("badchild.child.json"), "{")?;
    let memory = read_memory(root.path())?;
    assert_eq!(memory.len(), 5);
    assert!(memory.iter().all(|capture| !capture.attribution_complete()));
    let missing = memory
        .iter()
        .find(|item| item.capture.process_id == "absent")
        .ok_or("missing expected child")?;
    assert_eq!(missing.status, MemoryStatus::Missing);
    assert_eq!(
        missing
            .capture
            .observation
            .as_ref()
            .ok_or("parent observation")?
            .pid,
        Some(77)
    );
    let display = render_memory(&memory, 10);
    assert!(display.contains("overflow threads: 1"));
    assert!(display.contains("retained origin overflow allocations: 1"));
    let comparison = compare(&record(memory.clone())?, &record(memory)?, 0.0);
    assert!(!comparison.memory_complete());
    assert!(comparison.allocations().is_empty());
    assert!(
        render_comparison(&comparison, 10).contains("Memory attribution comparison: incomplete")
    );
    Ok(())
}

#[test]
fn comparisons_require_matching_process_repeat_operation_and_accounting_boundaries() -> Result {
    let root = tempfile::tempdir()?;
    let manifest = manifest(root.path());
    publish(&manifest, Some(&allocation()))?;
    let before = capture(&manifest);
    let mut after = before.clone();
    after
        .allocation
        .as_mut()
        .ok_or("allocation")?
        .functions
        .as_mut()
        .ok_or("rows")?[0]
        .bytes = Some(PRECISE + 10);
    let comparison = compare(
        &record(vec![before.clone()])?,
        &record(vec![after.clone()])?,
        0.0,
    );
    assert!(comparison.memory_complete());
    assert_eq!(comparison.allocations()[0].moved(), 10);
    assert!(render_comparison(&comparison, 10).contains("9007199254740993 B"));
    for variant in 0..6 {
        let mut changed = after.clone();
        match variant {
            0 => changed.capture.expected.as_mut().ok_or("identity")?.repeat = Some(2),
            1 => {
                changed
                    .capture
                    .manifest
                    .as_mut()
                    .ok_or("manifest")?
                    .accounting = "inclusive".into()
            }
            2 => {
                changed
                    .capture
                    .manifest
                    .as_mut()
                    .ok_or("manifest")?
                    .traffic_metric = "count".into()
            }
            3 => {
                changed
                    .allocation
                    .as_mut()
                    .ok_or("allocation")?
                    .functions
                    .as_mut()
                    .ok_or("rows")?[0]
                    .name = "different".into()
            }
            4 => changed.issues.push("depth limit hits: 1".into()),
            _ => {
                changed
                    .allocation
                    .as_mut()
                    .ok_or("allocation")?
                    .heap
                    .as_mut()
                    .ok_or("heap")?
                    .peak
                    .as_mut()
                    .ok_or("peak")?
                    .schema_version = 2
            }
        }
        let comparison = compare(&record(vec![before.clone()])?, &record(vec![changed])?, 0.0);
        assert!(comparison.allocations().is_empty(), "variant {variant}");
        assert!(!comparison.memory_complete());
    }
    let duplicates = compare(
        &record(vec![before.clone(), before.clone()])?,
        &record(vec![after.clone(), after])?,
        0.0,
    );
    assert!(duplicates.allocations().is_empty());
    assert!(!duplicates.memory_complete());
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
        .env_remove("CORDIAL_PROFILE_TICKET")
        .env_remove("CORDIAL_PERF_PROFILE_DIR")
        .status()?;
    assert!(status.success());
    let memory = read_memory(root.path())?;
    assert_eq!(memory.len(), 1);
    assert_eq!(memory[0].status, MemoryStatus::Available);
    let allocation = memory[0].allocation.as_ref().ok_or("allocation")?;
    let row = allocation
        .functions
        .as_ref()
        .ok_or("rows")?
        .iter()
        .find(|row| row.name.ends_with("::retained_fixture"))
        .ok_or("fixture function")?;
    assert_eq!(row.bytes, Some(12345));
    let origin = allocation
        .heap
        .as_ref()
        .ok_or("heap")?
        .origins
        .iter()
        .find(|origin| {
            origin
                .name
                .as_ref()
                .is_some_and(|name| name.ends_with("::retained_fixture"))
        })
        .ok_or("retained origin")?;
    assert_eq!(origin.live_bytes, 12345);
    assert!(
        !memory[0].attribution_complete(),
        "full program boundaries remain unverified"
    );
    let saved = record(memory)?;
    let store = PerfStore::new(&StoreLayout::from_root(
        root.path().join("store"),
        "fixture",
    ));
    let loaded = PerfStore::load(&store.save(&saved)?)?;
    assert_eq!(loaded, saved);
    let display = render_memory(loaded.memory(), usize::MAX);
    assert!(display.contains("bytes=Some(12345)"));
    assert!(display.contains("live=12345 B"));
    Ok(())
}

#[test]
#[cfg(all(unix, feature = "hotpath-alloc"))]
fn workflow_imports_exact_child_reports_without_a_driver_profile() -> Result {
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
            let row = capture
                .allocation
                .as_ref()
                .ok_or("allocation")?
                .functions
                .as_ref()
                .ok_or("rows")?
                .iter()
                .find(|row| row.name.ends_with("::retained_fixture"))
                .ok_or("fixture row")?;
            assert_eq!(row.bytes, Some(12345));
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
