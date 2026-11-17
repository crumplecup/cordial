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
        tracing::debug!(target: "salsa::state_size", ingredient = "ImplId", bytes = 4_096_u64, "share");
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
        value("state_size.implid.bytes"),
        Some(4_096),
        "an ingredient's share of the saved state"
    );
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

/// A run that is ended from outside its own flow (the memory limit stops it where it is) has still
/// said what it had done, and why it ended and what it was doing: that is when the numbers matter.
#[test]
fn a_run_ended_from_outside_its_flow_still_says_what_it_had_done() {
    let root = tempfile::tempdir().expect("a directory");
    let store = StoreLayout::from_root(root.path().to_path_buf(), "specimen".to_owned());
    cordial::keep_run_metrics_at(&store);
    cordial::write_ended_run_metrics("memory limit", "Saving the analysis state");
    let metrics = cordial::read_run_metrics(&store);
    assert_eq!(metrics.values().get("run.ended.memory_limit"), Some(&1));
    assert_eq!(
        metrics.notes().get("run.ended.during").map(String::as_str),
        Some("Saving the analysis state")
    );
    assert!(metrics.values().contains_key("run.queries_executed"));
}

/// With the steps asked for, the queries Salsa executes are counted by the step of cordial that
/// ran them and by the query, and the most executed are kept in the metrics.
#[test]
fn queries_are_counted_by_the_step_that_asked() {
    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(cordial::metrics_filter_with(true)));
    tracing::subscriber::with_default(subscriber, || {
        let step = tracing::debug_span!(target: "cordial::specimen", "capability");
        let _inside = step.enter();
        let helper = tracing::debug_span!(target: "cordial::specimen", "node");
        let _helping = helper.enter();
        for _ in 0..3 {
            tracing::info!(target: "salsa::function::execute", "ImplSignature::of_(Id(1))");
        }
        tracing::info!(target: "salsa::function::execute", "AttrFlags::query_(Id(2))");
    });
    tracing::info!(target: "salsa::function::execute", "never counted: nothing asked for it");
    let root = tempfile::tempdir().expect("a directory");
    let store = StoreLayout::from_root(root.path().to_path_buf(), "specimen".to_owned());
    cordial::write_run_metrics(&store).expect("the metrics are written");
    let metrics = cordial::read_run_metrics(&store);
    let value = |name: &str| metrics.values().get(name).copied();
    // The innermost step first, then the one that asked.
    assert_eq!(
        value("steps.node < capability > ImplSignature::of_"),
        Some(3)
    );
    assert_eq!(
        value("steps.node < capability > AttrFlags::query_"),
        Some(1)
    );
}

/// Without the steps asked for, the queries Salsa executes are not looked at: they would cost an
/// event each.
#[test]
fn queries_are_not_counted_unless_asked_for() {
    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(cordial::metrics_filter_with(false)));
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(target: "salsa::function::execute", "Unwatched::query_(Id(9))");
    });
    let root = tempfile::tempdir().expect("a directory");
    let store = StoreLayout::from_root(root.path().to_path_buf(), "specimen".to_owned());
    cordial::write_run_metrics(&store).expect("the metrics are written");
    let metrics = cordial::read_run_metrics(&store);
    assert!(
        metrics
            .values()
            .keys()
            .all(|name| !name.contains("Unwatched"))
    );
}

mod execution_time {
    use cordial::{MetricsLayer, StoreLayout};
    use salsa::{Database, Setter, Storage};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::SubscriberExt;

    #[salsa::input]
    struct Source {
        #[returns(copy)]
        value: u64,
    }

    #[salsa::db]
    #[derive(Clone, Default)]
    struct Db {
        storage: Storage<Self>,
    }

    #[salsa::db]
    impl Database for Db {}

    std::thread_local! {
        static HELD: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    }

    fn held() -> i64 {
        HELD.with(std::cell::Cell::get)
    }

    /// Takes a while on its own.
    #[salsa::tracked(returns(copy))]
    fn the_inner_one(db: &dyn Database, source: Source) -> u64 {
        std::thread::sleep(std::time::Duration::from_millis(60));
        HELD.with(|held| held.set(held.get() + 4096));
        source.value(db)
    }

    /// Takes little on its own and starts the inner one.
    #[salsa::tracked(returns(copy))]
    fn the_outer_one(db: &dyn Database, source: Source) -> u64 {
        the_inner_one(db, source) + 1
    }

    /// With the steps asked for, each query executed says how long it took on its own: what it
    /// started is counted for those, not again for it.
    #[test]
    fn a_query_executed_says_what_it_took_on_its_own() {
        salsa::set_allocation_probe(held);
        let subscriber = tracing_subscriber::registry()
            .with(MetricsLayer.with_filter(cordial::metrics_filter_with(true)));
        tracing::subscriber::with_default(subscriber, || {
            let mut db = Db::default();
            let source = Source::new(&db, 7);
            assert_eq!(the_outer_one(&db, source), 8);
            // A new revision, asked for by a step of the program, and the queries run again.
            let step = tracing::debug_span!(target: "cordial::specimen", "changing_the_input");
            let _inside = step.enter();
            source.set_value(&mut db).to(9);
            assert_eq!(the_outer_one(&db, source), 10);
        });
        let root = tempfile::tempdir().expect("a directory");
        let store = StoreLayout::from_root(root.path().to_path_buf(), "specimen".to_owned());
        cordial::write_run_metrics(&store).expect("the metrics are written");
        let metrics = cordial::read_run_metrics(&store);
        let value = |name: &str| {
            metrics
                .values()
                .iter()
                .find(|(key, _)| {
                    key.starts_with("execution.") && key.contains(name) && key.ends_with(".nanos")
                })
                .map(|(_, value)| *value)
        };
        let inner = value("the_inner_one").expect("the inner query is timed");
        let outer = value("the_outer_one").expect("the outer query is timed");
        assert!(inner >= 60_000_000, "{inner} ns for 60 ms of sleeping");
        assert!(
            outer < inner / 2,
            "outer {outer} ns counted the inner {inner} ns"
        );
        // The inner query holds 4 KiB more each time it is done (it ran twice), and the outer one
        // nothing of its own.
        let bytes = |query: &str| {
            metrics
                .values()
                .iter()
                .find(|(key, _)| {
                    key.starts_with("execution.")
                        && key.contains(query)
                        && key.ends_with(".retained")
                })
                .map(|(_, value)| *value)
        };
        // It ran twice, and the second time replaced the memo of the first.
        let value = |name: &str| metrics.values().get(name).copied();
        let again = |query: &str| {
            metrics
                .values()
                .iter()
                .find(|(key, _)| {
                    key.starts_with("execution.") && key.contains(query) && key.ends_with(".again")
                })
                .map(|(_, value)| *value)
        };
        assert_eq!(again("the_inner_one"), Some(1));
        assert_eq!(again("the_outer_one"), Some(1));
        // The step that asked for the revision is told.
        assert!(
            metrics.values().iter().any(|(key, count)| key
                .ends_with("changing_the_input > new_revision")
                && *count >= 1),
            "no step is told to have started a revision: {:?}",
            metrics
                .values()
                .keys()
                .filter(|key| key.starts_with("steps."))
                .collect::<Vec<_>>()
        );
        let _ = value;
        assert_eq!(bytes("the_inner_one"), Some(8192));
        assert_eq!(bytes("the_outer_one"), Some(0));
    }
}
