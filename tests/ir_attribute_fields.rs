//! Field-only reads must not expand unrelated packed evidence.
use cordial::{ATTR_SEMANTIC_FUNCTION, NodeKind, NodeWeight};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn weight(bytes: usize) -> NodeWeight {
    let mut weight = NodeWeight::new(NodeKind::Module);
    weight.set_attr(
        ATTR_SEMANTIC_FUNCTION,
        json!({"definition": 17, "unused": {"payload": "x".repeat(bytes)}}),
    );
    weight
}

fn lookup(weight: &NodeWeight) -> Option<Value> {
    weight.attr_field(ATTR_SEMANTIC_FUNCTION, "definition")
}

#[test]
fn cold_reads_skip_unrelated_fields_and_preserve_wire_values() -> Result {
    let weight = weight(256 * 1024);
    let wire = serde_json::to_vec(&weight)?;
    assert_eq!(lookup(&weight), Some(json!(17)));
    assert_eq!(weight.attr_field(ATTR_SEMANTIC_FUNCTION, "missing"), None);
    assert_eq!(weight.packed(), (1, 0));
    assert_eq!(serde_json::to_vec(&weight)?, wire);
    assert_eq!(weight.packed(), (1, 0));
    let restored: NodeWeight = serde_json::from_slice(&wire)?;
    assert_eq!(lookup(&restored), Some(json!(17)));
    assert_eq!(restored.packed(), (1, 0));
    assert_eq!(
        restored.attr(ATTR_SEMANTIC_FUNCTION),
        weight.attr(ATTR_SEMANTIC_FUNCTION)
    );
    Ok(())
}

#[test]
fn field_reads_match_json_for_warm_cold_and_unpacked_values() {
    for value in [
        Value::Null,
        json!(true),
        json!(12),
        json!("text"),
        json!([{"definition": 2}]),
        json!({}),
        json!({"definition": null}),
        json!({"definition": {"unknown": [1, "two", null]}, "unused": {"nested": [true]}}),
    ] {
        for key in [ATTR_SEMANTIC_FUNCTION, "unpacked"] {
            let mut weight = NodeWeight::new(NodeKind::Module);
            weight.set_attr(key, value.clone());
            assert_eq!(
                weight.attr_field(key, "definition"),
                value.get("definition").cloned()
            );
            assert_eq!(weight.attr_field(key, "absent"), None);
            assert_eq!(weight.attr(key), Some(&value));
            assert_eq!(
                weight.attr_field(key, "definition"),
                value.get("definition").cloned()
            );
        }
    }
}

#[test]
fn latest_attribute_controls_fields_and_clones_remain_independent() {
    let weight = weight(1024);
    assert_eq!(weight.attr_field("absent", "definition"), None);
    let _ = weight.attr(ATTR_SEMANTIC_FUNCTION);
    assert_eq!(weight.packed(), (1, 1));
    assert_eq!(lookup(&weight), Some(json!(17)));
    let mut cloned = weight.clone();
    assert_eq!(lookup(&cloned), Some(json!(17)));
    assert_eq!(cloned.packed(), (1, 0));
    for value in [json!({}), Value::Null, json!({"definition": 23})] {
        cloned.set_attr(ATTR_SEMANTIC_FUNCTION, value.clone());
        assert_eq!(lookup(&cloned), value.get("definition").cloned());
        assert_eq!(lookup(&weight), Some(json!(17)));
    }
    cloned.replace_attr(ATTR_SEMANTIC_FUNCTION, json!({"definition": 29}));
    assert_eq!(lookup(&cloned), Some(json!(29)));
    assert_eq!(weight.packed(), (1, 1));
}

#[cfg(feature = "hotpath-alloc")]
#[hotpath::measure]
fn small_field(weight: &NodeWeight) -> Option<Value> {
    lookup(weight)
}

#[cfg(feature = "hotpath-alloc")]
#[hotpath::measure]
fn large_field(weight: &NodeWeight) -> Option<Value> {
    lookup(weight)
}

#[cfg(feature = "hotpath-alloc")]
#[hotpath::measure]
fn repeated_fields(weight: &NodeWeight) {
    for _ in 0..16 {
        std::hint::black_box(lookup(weight));
    }
}

#[cfg(feature = "hotpath-alloc")]
#[test]
fn allocation_fixture() -> Result {
    let Some(root) = std::env::var_os("CORDIAL_ATTRIBUTE_FIELDS_FIXTURE") else {
        return Ok(());
    };
    let small = weight(16 * 1024);
    let large = weight(256 * 1024);
    let repeated = weight(256 * 1024);
    let mut capture = cordial::profiling::Capture::start(
        std::path::Path::new(&root),
        cordial::profiling::Identity::default(),
    )?;
    assert_eq!(small_field(&small), Some(json!(17)));
    assert_eq!(large_field(&large), Some(json!(17)));
    repeated_fields(&repeated);
    for weight in [&small, &large, &repeated] {
        assert_eq!(weight.packed(), (1, 0));
    }
    capture.finish(cordial::profiling::FinishReason::Normal)?;
    Ok(())
}

#[cfg(feature = "hotpath-alloc")]
#[test]
fn upstream_hotpath_measures_field_reads_without_unrelated_payload_growth() -> Result {
    let root = tempfile::tempdir()?;
    let output = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "allocation_fixture"])
        .env("CORDIAL_ATTRIBUTE_FIELDS_FIXTURE", root.path())
        .env("HOTPATH_ALLOC_CUMULATIVE", "1")
        .env("HOTPATH_ALLOC_METRIC", "bytes")
        .env("HOTPATH_METRICS_SERVER_OFF", "1")
        .env_remove("HOTPATH_OUTPUT_PATH")
        .env_remove("HOTPATH_OUTPUT_FORMAT")
        .env_remove("CORDIAL_PERF_PROFILE_DIR")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let captures = cordial::perf::read_captures(root.path())?;
    let manifest = captures[0].manifest.as_ref().ok_or("manifest")?;
    let report: Value = serde_json::from_slice(&std::fs::read(&manifest.report)?)?;
    let rows = report["functions_alloc"]["data"].as_array().ok_or("rows")?;
    for name in ["small_field", "large_field", "repeated_fields"] {
        let row = rows
            .iter()
            .find(|row| {
                row["name"]
                    .as_str()
                    .is_some_and(|path| path.rsplit("::").next() == Some(name))
            })
            .ok_or("row")?;
        assert_eq!(row["calls"], 1);
        eprintln!("{name}: {}", row["total"]);
        let total = cordial::perf::parse_bytes(row["total"].as_str().ok_or("total")?)
            .ok_or("allocation bytes")?;
        assert!(total < 8 * 1024, "{name}: {total} bytes");
    }
    Ok(())
}
