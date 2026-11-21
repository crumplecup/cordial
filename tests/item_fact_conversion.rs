//! A small shared-loader fixture for flow resolution and conversion traffic.
#![cfg(feature = "hotpath-alloc")]

use cordial::{CrateIr, CrateTarget, IrView, SourceCallResolution, SourceDatabase};
use serde_json::Value;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn check_flows(graph: &CrateIr) -> Result {
    let nodes = graph.nodes_matching(&cordial::BasicQuery::items());
    let target = nodes
        .iter()
        .find(|node| node.weight().name().as_deref() == Some("target"))
        .ok_or("target")?
        .id();
    let caller = nodes
        .iter()
        .find(|node| node.weight().name().as_deref() == Some("caller"))
        .ok_or("caller")?
        .id();
    let flow = graph.function_call_flow(caller)?.ok_or("flow")?;
    assert_eq!(flow.calls().len(), 64);
    let target_syntax = graph
        .function_source_syntax(target)?
        .ok_or("target syntax")?;
    let caller_syntax = graph
        .function_source_syntax(caller)?
        .ok_or("caller syntax")?;
    assert_eq!(target_syntax.calls().len(), 32);
    assert_eq!(flow.source_digest(), caller_syntax.source_digest());
    for (call, syntax) in flow.calls().iter().zip(caller_syntax.calls()) {
        assert_eq!(call.range(), syntax.range());
        assert_eq!(
            call.resolution(),
            &SourceCallResolution::Source {
                function: target,
                source_digest: target_syntax.source_digest(),
            }
        );
    }
    for node in nodes {
        let attrs = node.weight().attrs();
        let position = |key: &str| attrs.iter().position(|(name, _)| name == key);
        let syntax = position(cordial::ATTR_FUNCTION_SOURCE_SYNTAX).ok_or("syntax attribute")?;
        let semantic = position(cordial::ATTR_SEMANTIC_FUNCTION).ok_or("semantic attribute")?;
        let location = position(cordial::ATTR_FUNCTION_LOCATION).ok_or("location attribute")?;
        assert!(syntax < semantic && semantic < location);
    }
    Ok(())
}

#[test]
fn allocation_fixture() -> Result {
    let Some(root) = std::env::var_os("CORDIAL_ITEM_CONVERSION_FIXTURE") else {
        return Ok(());
    };
    let root = std::path::Path::new(&root);
    std::fs::create_dir(root.join("src"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='item_conversion'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )?;
    let source = format!(
        "pub fn leaf() {{}}\npub fn target() {{ {} }}\npub fn caller() {{ {} }}\n",
        "leaf();".repeat(32),
        "target();".repeat(64)
    );
    let path = root.join("src/lib.rs");
    std::fs::write(&path, source)?;
    let mut db = SourceDatabase::load(root)?;
    let mut capture =
        cordial::profiling::Capture::start(root, cordial::profiling::Identity::default())?;
    let graph = db.project(&CrateTarget::new("item_conversion", root), &[path])?;
    capture.finish(cordial::profiling::FinishReason::Normal)?;
    check_flows(&graph)?;
    Ok(())
}

#[test]
fn upstream_hotpath_measures_item_fact_conversion() -> Result {
    let root = tempfile::tempdir()?;
    let output = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "allocation_fixture"])
        .env("CORDIAL_ITEM_CONVERSION_FIXTURE", root.path())
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
    for name in ["resolve_flows", "collect_items"] {
        let row = rows
            .iter()
            .find(|row| {
                row["name"]
                    .as_str()
                    .is_some_and(|path| path.ends_with(&format!("::items::{name}")))
            })
            .ok_or("row")?;
        assert_eq!(row["calls"], 1);
        eprintln!("{name}: {}", row["total"]);
        let bytes =
            cordial::perf::parse_bytes(row["total"].as_str().ok_or("total")?).ok_or("bytes")?;
        if name == "resolve_flows" {
            // The old JSON reconstruction and per-call target copies reported
            // 932.6 KB. Leave room for rounded upstream output and containers.
            assert!(bytes < 64 * 1024, "flow resolution allocated {bytes} bytes");
        }
    }
    Ok(())
}
