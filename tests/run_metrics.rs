//! The numbers a run says about itself are added up as the events happen, kept under names that say
//! what they are about, and written to the store for the performance record.

use cordial::{MetricsLayer, StoreLayout, metrics_filter};
use tracing_subscriber::{Layer, layer::SubscriberExt};

#[test]
fn a_runs_events_are_added_up_by_what_they_are_about() {
    let subscriber =
        tracing_subscriber::registry().with(MetricsLayer.with_filter(metrics_filter()));
    tracing::subscriber::with_default(subscriber, || {
        for micros in [10_u64, 32] {
            tracing::info!(
                target: "salsa::persistence",
                ingredient = "persisted_evidence",
                memos = 5_u64,
                breadth = "a=1x2 (3)",
                micros,
                "flattened"
            );
        }
        tracing::info!(target: "salsa::persistence", ingredient = "def_map_fingerprint", memos = 7_u64, "flattened");
        tracing::info!(target: "cordial::phase_queries", phase = "Analysing cfg context 12", queries = 100_u64, verified = 4_u64, "phase");
        tracing::info!(target: "cordial::phase_queries", phase = "Analysing cfg context 13", queries = 20_u64, verified = 1_u64, "phase");
        tracing::info!(target: "cordial::audit", audited = 260_u64, mismatched = 0_u64, "audit");
        tracing::debug!(target: "cordial::cache", what = "a definition map", volatile = 0_u64, "fingerprint");
        tracing::debug!(target: "cordial::cache", what = "a definition map", volatile = 3_u64, reasons = ?[("a closure", 3_u32)], "fingerprint");
        tracing::info!(target: "cordial::semantic::fallback", step = "prime", "fell back");
        // What the layer is not asked about is not counted, and neither is a level below the one asked for.
        tracing::info!(target: "cordial::keys", queries = 99_u64, "elsewhere");
        tracing::debug!(target: "cordial::audit", audited = 1_000_u64, "too fine");
    });
    let root = tempfile::tempdir().expect("a directory");
    let store = StoreLayout::from_root(root.path().to_path_buf(), "specimen".to_owned());
    cordial::write_run_metrics(&store).expect("the metrics are written");
    let metrics = cordial::read_run_metrics(&store);
    let value = |name: &str| metrics.values().get(name).copied();
    assert_eq!(
        value("persistence.persisted_evidence.memos"),
        Some(10),
        "summed over events"
    );
    assert_eq!(value("persistence.persisted_evidence.micros"), Some(42));
    assert_eq!(value("persistence.persisted_evidence.events"), Some(2));
    assert_eq!(
        value("persistence.def_map_fingerprint.memos"),
        Some(7),
        "by ingredient"
    );
    assert_eq!(
        value("phase_queries.analysing_cfg_context.queries"),
        Some(120),
        "digits do not split a phase"
    );
    assert_eq!(value("phase_queries.analysing_cfg_context.events"), Some(2));
    assert_eq!(value("audit.audited"), Some(260));
    assert_eq!(value("audit.mismatched"), Some(0));
    assert_eq!(value("cache.a_definition_map.volatile"), Some(3));
    assert_eq!(
        value("cache.a_definition_map.events"),
        Some(2),
        "a fingerprint with nothing volatile is counted too"
    );
    assert_eq!(value("semantic.fallback.prime.events"), Some(1));
    assert_eq!(
        value("keys.queries"),
        None,
        "a target it is not asked about"
    );
    assert!(
        value("run.queries_executed").is_some(),
        "what the process knows of itself"
    );
    assert_eq!(
        metrics
            .notes()
            .get("persistence.persisted_evidence.breadth")
            .map(String::as_str),
        Some("a=1x2 (3)")
    );
    assert!(
        metrics
            .notes()
            .contains_key("cache.a_definition_map.reasons")
    );
}
