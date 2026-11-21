//! Typed evidence consumers share the stored encoding without retaining JSON.
use cordial::{
    ATTR_SEMANTIC_FUNCTION, CrateIr, IrView, NodeId, NodeKind, NodeWeight, SemanticFunction,
};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn function(bytes: usize) -> Value {
    json!({
        "definition": 17,
        "label": "x".repeat(bytes),
        "output": {"Other": "Unit"},
        "calls": [{"range": {"start": 2, "end": 7}, "target": {"Function": 19},
                   "output": "Unavailable"}],
        "reachable_result_errors": [],
        "incomplete": false
    })
}

fn fixture(bytes: usize) -> (CrateIr, NodeId) {
    let mut ir = CrateIr::new("evidence_reads");
    let mut weight = NodeWeight::new(NodeKind::Module);
    weight.set_attr(ATTR_SEMANTIC_FUNCTION, function(bytes));
    let node = ir.insert_node(weight);
    (ir, node)
}

#[test]
fn typed_reads_preserve_values_without_filling_or_clearing_json_caches() -> Result {
    let (ir, node) = fixture(1024);
    let expected: SemanticFunction = serde_json::from_value(function(1024))?;
    let weight = ir.node_weight(node).ok_or("node")?;
    for warm in [false, true] {
        if warm {
            assert!(weight.attr(ATTR_SEMANTIC_FUNCTION).is_some());
        }
        assert_eq!(ir.semantic_function(node)?, Some(expected.clone()));
        assert_eq!(weight.packed(), (1, usize::from(warm)));
        let restored = CrateIr::from_stage_bytes(&ir.to_stage_bytes()?)?;
        assert_eq!(restored.semantic_function(node)?, Some(expected.clone()));
        assert_eq!(
            restored.node_weight(node).ok_or("restored node")?.packed(),
            (1, 0)
        );
    }
    assert!(ir.semantic_function(NodeId::new(u32::MAX))?.is_none());
    assert!(ir.semantic_function(ir.root())?.is_none());
    Ok(())
}

#[test]
fn presence_queries_and_context_activity_do_not_decode_evidence() -> Result {
    let (mut ir, node) = fixture(1024);
    ir.set_attr(ir.root(), cordial::ATTR_SEMANTIC_SNAPSHOT, json!(null))?;
    let query = cordial::QueryBuilder::new()
        .has_attr(ATTR_SEMANTIC_FUNCTION)
        .build();
    assert_eq!(
        ir.nodes_matching(&query)
            .iter()
            .map(|node| node.id())
            .collect::<Vec<_>>(),
        [node]
    );
    let activity = ir.context_activity()?.ok_or("context activity")?;
    assert_eq!(
        activity.active(&ir, node)?,
        [cordial::SemanticContextKey::ordinary()]
    );
    assert!(!ir.node(node).ok_or("node")?.has_attr("absent"));
    assert_eq!(ir.node_weight(node).ok_or("node")?.packed(), (1, 0));
    let mut null = NodeWeight::new(NodeKind::Module);
    null.set_attr(ATTR_SEMANTIC_FUNCTION, json!(null));
    assert!(null.has_attr(ATTR_SEMANTIC_FUNCTION));
    assert_eq!(null.packed(), (1, 0));
    Ok(())
}

#[test]
fn context_reads_keep_contexts_distinct_and_repeated_keys_use_the_latest_value() -> Result {
    let (mut ir, node) = fixture(1024);
    let contexts = json!([
        {"context": {"enabled": ["test"], "disabled": []}, "function": function(16)},
        {"context": {"enabled": ["unix"], "disabled": ["test"]}, "function": function(32)}
    ]);
    ir.set_attr(node, cordial::ATTR_SEMANTIC_CONTEXT_FUNCTIONS, json!(null))?;
    ir.set_attr(
        node,
        cordial::ATTR_SEMANTIC_CONTEXT_FUNCTIONS,
        contexts.clone(),
    )?;
    let expected: Vec<cordial::SemanticContextFunction> = serde_json::from_value(contexts)?;
    assert_eq!(ir.semantic_context_functions(node)?, expected);
    assert_eq!(ir.node_weight(node).ok_or("node")?.packed(), (3, 0));
    assert_eq!(
        ir.semantic_function(node)?.ok_or("function")?.label().len(),
        1024
    );
    Ok(())
}

fn context_fixture(bytes: usize) -> Result<(CrateIr, NodeId)> {
    let mut ir = CrateIr::new("context_reads");
    let node = ir.insert_node(NodeWeight::new(NodeKind::Module));
    ir.set_attr(
        node,
        cordial::ATTR_SEMANTIC_CONTEXT_FUNCTIONS,
        json!([
            {"context": {"enabled": ["kani"], "disabled": []}, "function": function(bytes)},
            {"context": {"enabled": ["test"], "disabled": []}, "function": function(32)}
        ]),
    )?;
    Ok((ir, node))
}

#[test]
fn context_fallback_preserves_order_ordinary_precedence_and_cache_isolation() -> Result {
    let (mut ir, node) = context_fixture(1024)?;
    let expected: SemanticFunction = serde_json::from_value(function(1024))?;
    assert_eq!(
        ir.semantic_function_or_first_context(node)?,
        Some(expected.clone())
    );
    assert_eq!(ir.node_weight(node).ok_or("node")?.packed(), (1, 0));
    let restored = CrateIr::from_stage_bytes(&ir.to_stage_bytes()?)?;
    assert_eq!(
        restored.semantic_function_or_first_context(node)?,
        Some(expected)
    );
    assert!(
        ir.node(node)
            .ok_or("node")?
            .attr(cordial::ATTR_SEMANTIC_CONTEXT_FUNCTIONS)
            .is_some()
    );
    assert_eq!(
        ir.semantic_function_or_first_context(node)?
            .ok_or("facts")?
            .label()
            .len(),
        1024
    );
    assert_eq!(ir.node_weight(node).ok_or("node")?.packed(), (1, 1));
    ir.set_attr(node, ATTR_SEMANTIC_FUNCTION, function(16))?;
    ir.set_attr(
        node,
        cordial::ATTR_SEMANTIC_CONTEXT_FUNCTIONS,
        json!("ignored because ordinary"),
    )?;
    assert_eq!(
        ir.semantic_function_or_first_context(node)?
            .ok_or("ordinary")?
            .label()
            .len(),
        16
    );
    assert!(ir.semantic_function_or_first_context(ir.root())?.is_none());
    Ok(())
}

#[test]
fn context_fallback_validates_all_entries_and_does_not_hide_malformed_ordinary_facts() -> Result {
    let (mut ir, node) = context_fixture(16)?;
    ir.set_attr(
        node,
        cordial::ATTR_SEMANTIC_CONTEXT_FUNCTIONS,
        json!([
            {"context": {"enabled": ["kani"], "disabled": []}, "function": function(16)},
            {"context": {"enabled": ["test"], "disabled": []}, "function": {}}
        ]),
    )?;
    assert!(ir.semantic_function_or_first_context(node).is_err());
    ir.set_attr(node, cordial::ATTR_SEMANTIC_CONTEXT_FUNCTIONS, json!([]))?;
    assert!(ir.semantic_function_or_first_context(node)?.is_none());
    ir.set_attr(node, ATTR_SEMANTIC_FUNCTION, json!(null))?;
    assert!(ir.semantic_function_or_first_context(node).is_err());
    Ok(())
}

#[test]
fn malformed_evidence_preserves_the_json_validation_error() -> Result {
    for value in [
        json!(null),
        json!("bad"),
        json!({}),
        json!({"definition": "bad"}),
    ] {
        let mut ir = CrateIr::new("malformed");
        ir.set_attr(ir.root(), ATTR_SEMANTIC_FUNCTION, function(16))?;
        ir.set_attr(ir.root(), ATTR_SEMANTIC_FUNCTION, value.clone())?;
        let expected = serde_json::from_value::<SemanticFunction>(value)
            .err()
            .ok_or("expected error")?;
        let actual = ir
            .semantic_function(ir.root())
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
    Ok(())
}

#[cfg(feature = "hotpath-alloc")]
#[hotpath::measure]
fn typed_consumer(ir: &CrateIr, node: NodeId) -> cordial::CordialResult<Option<SemanticFunction>> {
    ir.semantic_function(node)
}

#[cfg(feature = "hotpath-alloc")]
#[hotpath::measure]
fn consumer_stages(ir: &mut CrateIr, node: NodeId) -> cordial::CordialResult<()> {
    for _ in 0..4 {
        std::hint::black_box(ir.semantic_function(node)?);
        ir.drop_decoded_attrs();
    }
    Ok(())
}

#[cfg(feature = "hotpath-alloc")]
#[hotpath::measure]
fn context_consumer(
    ir: &CrateIr,
    node: NodeId,
) -> cordial::CordialResult<Option<SemanticFunction>> {
    ir.semantic_function_or_first_context(node)
}

#[cfg(feature = "hotpath-alloc")]
#[test]
fn allocation_fixture() -> Result {
    let Some(root) = std::env::var_os("CORDIAL_EVIDENCE_READS_FIXTURE") else {
        return Ok(());
    };
    let (ir, node) = fixture(256 * 1024);
    let (mut stages, stage_node) = fixture(256 * 1024);
    let (contexts, context_node) = context_fixture(256 * 1024)?;
    let mut capture = cordial::profiling::Capture::start(
        std::path::Path::new(&root),
        cordial::profiling::Identity::default(),
    )?;
    let result = typed_consumer(&ir, node)?.ok_or("function")?;
    consumer_stages(&mut stages, stage_node)?;
    let context_result = context_consumer(&contexts, context_node)?.ok_or("context facts")?;
    capture.finish(cordial::profiling::FinishReason::Normal)?;
    assert_eq!(result.label().len(), 256 * 1024);
    assert_eq!(result.calls().len(), 1);
    assert_eq!(context_result.label().len(), 256 * 1024);
    assert_eq!(
        contexts
            .node_weight(context_node)
            .ok_or("context node")?
            .packed(),
        (1, 0)
    );
    assert_eq!(ir.node_weight(node).ok_or("node")?.packed(), (1, 0));
    Ok(())
}

#[cfg(feature = "hotpath-alloc")]
#[test]
fn upstream_hotpath_measures_typed_evidence_reads() -> Result {
    let root = tempfile::tempdir()?;
    let output = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "allocation_fixture"])
        .env("CORDIAL_EVIDENCE_READS_FIXTURE", root.path())
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
    for name in ["typed_consumer", "consumer_stages", "context_consumer"] {
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
        let stages = if name == "consumer_stages" { 4 } else { 1 };
        assert!(
            bytes < stages * 300 * 1024,
            "{name} allocated {bytes} bytes"
        );
    }
    Ok(())
}
