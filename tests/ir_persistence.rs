//! Persistence fixtures only; no workspace or Cordial command is executed.
use cordial::{CrateIr, EdgeKind, ItemKind, NodeKind, NodeWeight, SemanticContextKey};
use serde_json::{Value, json};
#[cfg(feature = "hotpath-alloc")]
use std::path::Path;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn fixture() -> Result<CrateIr> {
    let mut ir = CrateIr::new("persistence_fixture");
    let mut caller = NodeWeight::new(NodeKind::Item(ItemKind::Fn)).with_name("caller");
    caller.set_attr("qualified_path", json!("persistence_fixture::caller"));
    caller.set_attr(
        cordial::ATTR_SEMANTIC_FUNCTION,
        json!({"label":"old","calls":[1]}),
    );
    caller.set_attr(
        cordial::ATTR_SEMANTIC_FUNCTION,
        json!({"label":"caller","calls":[2,3]}),
    );
    let caller = ir.insert_node(caller);
    let callee = ir.insert_node(NodeWeight::new(NodeKind::Item(ItemKind::Fn)).with_name("callee"));
    ir.insert_edge(ir.root(), caller, EdgeKind::Contains)?;
    ir.insert_edge(caller, callee, EdgeKind::Calls)?;
    ir.insert_edge(callee, caller, EdgeKind::Calls)?;
    ir.insert_context_call(caller, callee, SemanticContextKey::new(["test"], ["unix"]))?;
    Ok(ir)
}

fn decoded(bytes: &[u8]) -> Result<Value> {
    Ok(serde_json::from_slice(&zstd::stream::decode_all(bytes)?)?)
}

fn wire(ir: &CrateIr) -> Result<Value> {
    Ok(serde_json::to_value(ir.snapshot()?)?)
}

#[test]
fn borrowed_file_and_stage_writes_preserve_the_owned_snapshot_wire_and_ids() -> Result {
    for ir in [CrateIr::new("empty"), fixture()?] {
        let root = tempfile::tempdir()?;
        let expected = wire(&ir)?;
        let old_path = root.path().join("owned.zst");
        ir.snapshot()?.write_cache(&old_path)?;
        let new_path = root.path().join("borrowed.zst");
        ir.write_cache(&new_path)?;
        assert_eq!(decoded(&std::fs::read(&old_path)?)?, expected);
        assert_eq!(decoded(&std::fs::read(&new_path)?)?, expected);
        let staged = ir.to_stage_bytes()?;
        assert_eq!(decoded(&staged)?, expected);
        for read in [
            CrateIr::read_cache(&old_path)?,
            CrateIr::read_cache(&new_path)?,
            CrateIr::from_stage_bytes(&staged)?,
        ] {
            assert_eq!(read.root(), ir.root());
            assert_eq!(wire(&read)?, expected);
            assert_eq!(read.indexes().by_path(), ir.indexes().by_path());
            assert_eq!(read.indexes().by_kind(), ir.indexes().by_kind());
        }
    }
    Ok(())
}

#[test]
fn persistence_does_not_fill_or_clear_the_live_graphs_decoded_attribute_caches() -> Result {
    let ir = fixture()?;
    let caller = *ir
        .indexes()
        .by_path()
        .get("persistence_fixture::caller")
        .ok_or("caller")?;
    let node = ir.node_weight(caller).ok_or("node")?;
    assert_eq!(node.packed(), (2, 0));
    let root = tempfile::tempdir()?;
    for decoded_before in [0, 1] {
        if decoded_before == 1 {
            assert_eq!(
                node.attr(cordial::ATTR_SEMANTIC_FUNCTION).ok_or("facts")?["label"],
                "caller"
            );
        }
        let staged = ir.to_stage_bytes()?;
        ir.write_cache(&root.path().join("graph.zst"))?;
        assert_eq!(node.packed(), (2, decoded_before));
        let read = CrateIr::from_stage_bytes(&staged)?;
        let read_node = read.node_weight(caller).ok_or("read node")?;
        assert_eq!(read_node.packed(), (2, 0));
        let attrs = serde_json::to_value(read_node)?["attrs"].clone();
        assert_eq!(attrs[1][1]["label"], "old");
        assert_eq!(attrs[2][1]["label"], "caller");
        assert_eq!(read_node.packed(), (2, 0));
    }
    Ok(())
}

#[test]
fn cache_publication_is_atomic_and_failed_publication_removes_the_temporary_file() -> Result {
    let root = tempfile::tempdir()?;
    let path = root.path().join("graph.zst");
    let old = CrateIr::new("old");
    cordial::testing::write_cached_ir(&path, &old)?;
    assert_eq!(wire(&CrateIr::read_cache(&path)?)?, wire(&old)?);
    let ir = fixture()?;
    cordial::testing::write_cached_ir(&path, &ir)?;
    assert_eq!(wire(&CrateIr::read_cache(&path)?)?, wire(&ir)?);
    let blocked = root.path().join("blocked");
    std::fs::create_dir(&blocked)?;
    std::fs::write(blocked.join("marker"), b"preserved")?;
    assert!(cordial::testing::write_cached_ir(&blocked, &ir).is_err());
    assert_eq!(std::fs::read(blocked.join("marker"))?, b"preserved");
    assert_eq!(std::fs::read_dir(root.path())?.count(), 2);
    assert!(
        cordial::testing::write_cached_ir(&root.path().join("missing/graph.zst"), &ir).is_err()
    );
    assert_eq!(wire(&CrateIr::read_cache(&path)?)?, wire(&ir)?);
    assert!(CrateIr::from_stage_bytes(b"partial").is_err());
    Ok(())
}

#[cfg(feature = "hotpath-alloc")]
#[hotpath::measure]
fn owned_write(ir: &CrateIr, path: &Path) -> cordial::CordialResult<()> {
    ir.snapshot()?.write_cache(path)
}

#[cfg(feature = "hotpath-alloc")]
#[hotpath::measure]
fn borrowed_write(ir: &CrateIr, path: &Path) -> cordial::CordialResult<()> {
    ir.write_cache(path)
}

#[cfg(feature = "hotpath-alloc")]
#[test]
fn allocation_fixture() -> Result {
    let Some(root) = std::env::var_os("CORDIAL_IR_PERSISTENCE_FIXTURE") else {
        return Ok(());
    };
    let root = Path::new(&root);
    let mut ir = CrateIr::new("allocation_fixture");
    for _ in 0..64 {
        let mut node = NodeWeight::new(NodeKind::Module);
        node.set_attr("payload", json!("x".repeat(64 * 1024)));
        ir.insert_node(node);
    }
    let mut capture =
        cordial::profiling::Capture::start(root, cordial::profiling::Identity::default())?;
    owned_write(&ir, &root.join("owned.zst"))?;
    borrowed_write(&ir, &root.join("borrowed.zst"))?;
    capture.finish(cordial::profiling::FinishReason::Normal)?;
    Ok(())
}

#[cfg(feature = "hotpath-alloc")]
#[test]
fn upstream_hotpath_reports_the_removed_payload_copy() -> Result {
    let root = tempfile::tempdir()?;
    let output = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "allocation_fixture"])
        .env("CORDIAL_IR_PERSISTENCE_FIXTURE", root.path())
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
    let traffic = |suffix: &str| -> Result<(String, u64)> {
        let row = rows
            .iter()
            .find(|row| {
                row["name"]
                    .as_str()
                    .is_some_and(|name| name.ends_with(suffix))
            })
            .ok_or("row")?;
        assert_eq!(row["calls"], 1);
        let text = row["total"].as_str().ok_or("total")?.to_owned();
        let bytes = cordial::perf::parse_bytes(&text).ok_or("bytes")?;
        Ok((text, bytes))
    };
    let (owned_text, owned) = traffic("::owned_write")?;
    let (borrowed_text, borrowed) = traffic("::borrowed_write")?;
    eprintln!(
        "Upstream cumulative traffic, 4 MiB payload: owned {owned_text}; borrowed {borrowed_text}"
    );
    assert!(
        owned > borrowed + 2 * 1024 * 1024,
        "owned={owned}, borrowed={borrowed}"
    );
    assert_eq!(
        decoded(&std::fs::read(root.path().join("owned.zst"))?)?,
        decoded(&std::fs::read(root.path().join("borrowed.zst"))?)?
    );
    Ok(())
}
