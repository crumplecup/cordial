//! Allocation traffic for repeated reads of the shared loader's policy inventory.
#![cfg(feature = "allows")]

use cordial::{ATTR_SOURCE_POLICY_INVENTORY, CrateIr, IrView, SourcePolicyInventory};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn payload() -> Result<Value> {
    let mut value = serde_json::to_value(SourcePolicyInventory::default())?;
    value["snapshots"] = json!(
        (0..64)
            .map(|index| json!({
                "path": format!("{index}/{}", "x".repeat(4096)),
                "digest": vec![0u8; 32],
            }))
            .collect::<Vec<_>>()
    );
    Ok(value)
}

fn fixture(value: Value) -> Result<CrateIr> {
    let mut ir = CrateIr::new("policy_sharing");
    ir.set_attr(ir.root(), ATTR_SOURCE_POLICY_INVENTORY, value)?;
    Ok(ir)
}

#[test]
fn typed_policy_reads_keep_packed_bytes_cold_and_preserve_unknown_wire_fields() -> Result {
    let mut value = payload()?;
    value["future_inventory_field"] = json!({"keep": [1, 2, 3]});
    value["snapshots"][0]["future_snapshot_field"] = json!("keep this too");
    let expected: SourcePolicyInventory = serde_json::from_value(value.clone())?;
    let ir = fixture(value.clone())?;
    for _ in 0..3 {
        assert_eq!(ir.source_policy_inventory()?, Some(expected.clone()));
    }
    let weight = ir.node_weight(ir.root()).ok_or("root")?;
    assert_eq!(weight.packed(), (1, 0));
    assert_eq!(
        serde_json::to_value(weight)?["attrs"],
        json!([[ATTR_SOURCE_POLICY_INVENTORY, value]])
    );
    let restored: cordial::NodeWeight = rmp_serde::from_slice(&rmp_serde::to_vec(weight)?)?;
    assert_eq!(restored.attr(ATTR_SOURCE_POLICY_INVENTORY), Some(&value));
    assert_eq!(weight.packed(), (1, 0));
    let restored = CrateIr::from_stage_bytes(&ir.to_stage_bytes()?)?;
    assert_eq!(restored.source_policy_inventory()?, Some(expected));
    assert_eq!(
        restored
            .node_weight(restored.root())
            .ok_or("root")?
            .packed(),
        (1, 0)
    );
    Ok(())
}

#[test]
fn policy_clones_keep_cache_and_mutation_isolation_and_latest_error_behavior() -> Result {
    let original = payload()?;
    let mut ir = fixture(original.clone())?;
    assert_eq!(
        ir.node_weight(ir.root())
            .ok_or("root")?
            .attr(ATTR_SOURCE_POLICY_INVENTORY),
        Some(&original)
    );
    let clone = ir.clone();
    assert_eq!(
        clone
            .node_weight(clone.root())
            .ok_or("clone root")?
            .packed(),
        (1, 0)
    );
    assert_eq!(ir.drop_decoded_attrs(), 1);
    assert_eq!(ir.node_weight(ir.root()).ok_or("root")?.packed(), (1, 0));

    let mut malformed = original;
    malformed["snapshots"] = json!("invalid");
    let expected = serde_json::from_value::<SourcePolicyInventory>(malformed.clone())
        .err()
        .ok_or("expected error")?;
    ir.set_attr(ir.root(), ATTR_SOURCE_POLICY_INVENTORY, malformed)?;
    let actual = ir
        .source_policy_inventory()
        .err()
        .ok_or("read should fail")?;
    let mut cause: &(dyn std::error::Error + 'static) = &actual;
    while let Some(source) = cause.source() {
        cause = source;
    }
    assert_eq!(
        cause
            .downcast_ref::<serde_json::Error>()
            .ok_or("JSON cause")?
            .to_string(),
        expected.to_string()
    );
    assert_eq!(
        clone
            .source_policy_inventory()?
            .ok_or("cloned inventory")?
            .snapshots()
            .len(),
        64
    );
    assert!(CrateIr::new("missing").source_policy_inventory()?.is_none());
    Ok(())
}

#[test]
fn shared_consumers_reuse_one_inventory_and_release_it_without_losing_wire_data() -> Result {
    use std::sync::Arc;
    let mut value = payload()?;
    value["future"] = json!({"nested": [1, 2, 3]});
    let mut ir = fixture(value.clone())?;
    let first = ir.shared_source_policy_inventory()?.ok_or("first")?;
    for _ in 0..10 {
        let next = ir.shared_source_policy_inventory()?.ok_or("next")?;
        assert!(Arc::ptr_eq(&first, &next));
    }
    assert_eq!(ir.node_weight(ir.root()).ok_or("root")?.packed(), (1, 1));
    let restored = CrateIr::from_stage_bytes(&ir.to_stage_bytes()?)?;
    assert_eq!(
        restored
            .node_weight(restored.root())
            .ok_or("restored")?
            .packed(),
        (1, 0)
    );
    assert_eq!(
        *restored
            .shared_source_policy_inventory()?
            .ok_or("restored inventory")?,
        *first
    );
    let old = Arc::downgrade(&first);
    assert_eq!(ir.drop_decoded_attrs(), 1);
    assert_eq!(first.snapshots().len(), 64);
    assert_eq!(ir.node_weight(ir.root()).ok_or("root")?.packed(), (1, 0));
    let second = ir.shared_source_policy_inventory()?.ok_or("second")?;
    assert_eq!(*second, *first);
    assert!(!Arc::ptr_eq(&first, &second));
    drop(first);
    assert!(old.upgrade().is_none());
    assert_eq!(
        ir.node_weight(ir.root())
            .ok_or("root")?
            .attr(ATTR_SOURCE_POLICY_INVENTORY),
        Some(&value)
    );
    let weak = Arc::downgrade(&second);
    drop(second);
    assert_eq!(ir.drop_decoded_attrs(), 1); // Both representations count as one attribute.
    assert!(weak.upgrade().is_none());
    assert_eq!(ir.drop_decoded_attrs(), 0);
    Ok(())
}

#[test]
fn shared_policy_clones_start_cold_and_latest_malformed_values_cannot_reuse_old_facts() -> Result {
    use std::sync::Arc;
    let mut ir = fixture(payload()?)?;
    let first = ir.shared_source_policy_inventory()?.ok_or("first")?;
    let clone = ir.clone();
    assert_eq!(
        clone.node_weight(clone.root()).ok_or("clone")?.packed(),
        (1, 0)
    );
    let cloned = clone.shared_source_policy_inventory()?.ok_or("cloned")?;
    assert_eq!(*first, *cloned);
    assert!(!Arc::ptr_eq(&first, &cloned));
    for field in ["snapshots", "allows"] {
        let mut malformed = payload()?;
        malformed[field] = json!("bad");
        let expected = serde_json::from_value::<SourcePolicyInventory>(malformed.clone())
            .err()
            .ok_or("expected error")?;
        ir.set_attr(ir.root(), ATTR_SOURCE_POLICY_INVENTORY, malformed)?;
        let actual = ir
            .shared_source_policy_inventory()
            .err()
            .ok_or("read should fail")?;
        let mut cause: &(dyn std::error::Error + 'static) = &actual;
        while let Some(source) = cause.source() {
            cause = source;
        }
        assert_eq!(
            cause
                .downcast_ref::<serde_json::Error>()
                .ok_or("JSON cause")?
                .to_string(),
            expected.to_string()
        );
    }
    ir.set_attr(
        ir.root(),
        ATTR_SOURCE_POLICY_INVENTORY,
        serde_json::to_value(SourcePolicyInventory::default())?,
    )?;
    assert!(
        ir.shared_source_policy_inventory()?
            .ok_or("latest")?
            .snapshots()
            .is_empty()
    );
    assert_eq!(first.snapshots().len(), 64);
    assert!(
        CrateIr::new("missing")
            .shared_source_policy_inventory()?
            .is_none()
    );
    Ok(())
}

#[test]
fn concurrent_shared_readers_observe_the_same_validated_inventory() -> Result {
    let ir = fixture(payload()?)?;
    std::thread::scope(|scope| -> Result {
        let threads: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| ir.shared_source_policy_inventory()))
            .collect();
        let mut inventories = Vec::new();
        for thread in threads {
            inventories.push(
                thread
                    .join()
                    .map_err(|_| "thread panicked")??
                    .ok_or("inventory")?,
            );
        }
        for inventory in &inventories {
            assert!(std::sync::Arc::ptr_eq(&inventories[0], inventory));
        }
        Ok(())
    })
}

#[cfg(feature = "hotpath-alloc")]
fn read_ten(ir: &CrateIr) -> cordial::CordialResult<()> {
    for _ in 0..10 {
        let inventory = ir.shared_source_policy_inventory()?;
        std::hint::black_box(inventory);
    }
    Ok(())
}

#[cfg(feature = "hotpath-alloc")]
#[hotpath::measure]
fn consumer_reads(ir: &CrateIr) -> cordial::CordialResult<()> {
    read_ten(ir)
}

#[cfg(feature = "hotpath-alloc")]
#[hotpath::measure]
fn construction_and_reads(value: Value) -> Result<CrateIr> {
    let ir = fixture(value)?;
    read_ten(&ir)?;
    Ok(ir)
}

#[cfg(feature = "hotpath-alloc")]
#[hotpath::measure]
fn graph_clones(ir: &CrateIr) -> Vec<CrateIr> {
    (0..4).map(|_| std::hint::black_box(ir).clone()).collect()
}

#[cfg(feature = "hotpath-alloc")]
#[test]
fn allocation_fixture() -> Result {
    let Some(root) = std::env::var_os("CORDIAL_POLICY_SHARING_FIXTURE") else {
        return Ok(());
    };
    let ir = fixture(payload()?)?;
    let value = payload()?;
    let mut capture = cordial::profiling::Capture::start(
        std::path::Path::new(&root),
        cordial::profiling::Identity::default(),
    )?;
    consumer_reads(&ir)?;
    assert_eq!(ir.node_weight(ir.root()).ok_or("root")?.packed(), (1, 1));
    let built = construction_and_reads(value)?;
    let clones = graph_clones(&ir);
    std::hint::black_box(&clones);
    capture.finish(cordial::profiling::FinishReason::Normal)?;
    assert_eq!(
        built
            .source_policy_inventory()?
            .ok_or("inventory")?
            .snapshots()
            .len(),
        64
    );
    Ok(())
}

#[cfg(feature = "hotpath-alloc")]
#[test]
fn upstream_hotpath_measures_policy_construction_and_consumers() -> Result {
    let root = tempfile::tempdir()?;
    let output = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "allocation_fixture"])
        .env("CORDIAL_POLICY_SHARING_FIXTURE", root.path())
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
    for name in ["consumer_reads", "construction_and_reads", "graph_clones"] {
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
        let bytes =
            cordial::perf::parse_bytes(row["total"].as_str().ok_or("total")?).ok_or("bytes")?;
        let limit = match name {
            "graph_clones" => 32 * 1024,
            "consumer_reads" => 300 * 1024,
            _ => 1200 * 1024,
        };
        assert!(bytes < limit, "{name} allocated {bytes} bytes");
    }
    Ok(())
}
