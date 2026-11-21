//! Allocation traffic for node clones, using published Hotpath and small fixtures only.
#![cfg(feature = "hotpath-alloc")]

use cordial::{ATTR_SEMANTIC_FUNCTION, NodeKind, NodeWeight};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn weight(bytes: usize) -> NodeWeight {
    let mut node = NodeWeight::new(NodeKind::Module);
    node.set_attr(
        ATTR_SEMANTIC_FUNCTION,
        json!({"payload": "x".repeat(bytes)}),
    );
    node
}

fn clones(node: &NodeWeight, count: usize) -> Vec<NodeWeight> {
    (0..count)
        .map(|_| std::hint::black_box(node).clone())
        .collect()
}

#[hotpath::measure]
fn small_few(node: &NodeWeight) -> Vec<NodeWeight> {
    clones(node, 4)
}

#[hotpath::measure]
fn large_few(node: &NodeWeight) -> Vec<NodeWeight> {
    clones(node, 4)
}

#[hotpath::measure]
fn large_many(node: &NodeWeight) -> Vec<NodeWeight> {
    clones(node, 16)
}

#[hotpath::measure]
fn warm_many(node: &NodeWeight) -> Vec<NodeWeight> {
    clones(node, 16)
}

#[test]
fn allocation_fixture() -> Result {
    let Some(root) = std::env::var_os("CORDIAL_ATTRIBUTE_SHARING_FIXTURE") else {
        return Ok(());
    };
    let small = weight(16 * 1024);
    let large = weight(256 * 1024);
    let warm = large.clone();
    assert!(warm.attr(ATTR_SEMANTIC_FUNCTION).is_some());
    let mut capture = cordial::profiling::Capture::start(
        std::path::Path::new(&root),
        cordial::profiling::Identity::default(),
    )?;
    let copies = [
        small_few(&small),
        large_few(&large),
        large_many(&large),
        warm_many(&warm),
    ];
    std::hint::black_box(&copies);
    capture.finish(cordial::profiling::FinishReason::Normal)?;
    Ok(())
}

#[test]
fn upstream_hotpath_measures_clone_payload_and_consumer_growth() -> Result {
    let root = tempfile::tempdir()?;
    let output = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "allocation_fixture"])
        .env("CORDIAL_ATTRIBUTE_SHARING_FIXTURE", root.path())
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
    let traffic = |name| -> Result<u64> {
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
        Ok(cordial::perf::parse_bytes(row["total"].as_str().ok_or("total")?).ok_or("bytes")?)
    };
    let small_few = traffic("small_few")?;
    let large_few = traffic("large_few")?;
    let large_many = traffic("large_many")?;
    let warm_many = traffic("warm_many")?;
    // Rounded upstream strings allow some slack, but no payload-sized clone
    // allocation may return as payload size, fanout or source cache warmth grows.
    assert!(small_few.abs_diff(large_few) < 1024);
    assert!(large_many < 32 * 1024);
    assert!(large_many > large_few);
    assert!(warm_many.abs_diff(large_many) < 1024);
    Ok(())
}
