//! The live status counts Salsa's query executions and names the latest, whatever the log's
//! own filter lets through.

use cordial::{LiveLayer, LiveStatus, live_filter};
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;

#[test]
fn executed_queries_are_counted_and_the_latest_is_named() {
    // The log's filter shows nothing, as the CLI's does by default.
    let subscriber = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer().with_filter(tracing_subscriber::EnvFilter::new("off")),
        )
        .with(LiveLayer.with_filter(live_filter()));
    let _guard = tracing::subscriber::set_default(subscriber);

    let before = LiveStatus::global().executed();
    for number in 0..256 {
        tracing::info!(
            target: "salsa::function::execute",
            "live_probe(Id({number})): executing query"
        );
    }
    // Other targets are not queries.
    tracing::info!(target: "cordial::cache", "live_probe(Id(0)): executing query");

    assert_eq!(LiveStatus::global().executed() - before, 256);
    let line = LiveStatus::global().line();
    assert!(line.contains("queries"), "{line}");
    assert!(line.contains("latest live_probe"), "{line}");
}

/// What `cordial::phase_queries` events said: phase, total and the top of the list.
#[derive(Default, Clone)]
struct PhaseReports(std::sync::Arc<std::sync::Mutex<Vec<(String, u64, String)>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for PhaseReports {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().target() != "cordial::phase_queries" {
            return;
        }
        #[derive(Default)]
        struct Fields(String, u64, String);
        impl tracing::field::Visit for Fields {
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                if field.name() == "queries" {
                    self.1 = value;
                }
            }
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "phase" {
                    self.0 = value.to_owned();
                }
            }
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "top" {
                    self.2 = format!("{value:?}");
                }
            }
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        if let Ok(mut reports) = self.0.lock() {
            reports.push((fields.0, fields.1, fields.2));
        }
    }
}

#[test]
fn a_phase_that_ends_says_which_queries_it_executed_most() {
    let reports = PhaseReports::default();
    let subscriber = tracing_subscriber::registry()
        .with(reports.clone())
        .with(LiveLayer.with_filter(live_filter()));
    let _guard = tracing::subscriber::set_default(subscriber);

    let status = LiveStatus::global();
    status.set_phase("phase probe one");
    // Whatever earlier tests in this process counted is reported with this phase's start.
    status.set_phase("phase probe two");
    reports
        .0
        .lock()
        .map(|mut seen| seen.clear())
        .unwrap_or_default();
    for _ in 0..30 {
        tracing::info!(target: "salsa::function::execute", "parse_probe(Id(1)): executing query");
    }
    for _ in 0..5 {
        tracing::info!(target: "salsa::function::execute", "infer_probe(Id(2)): executing query");
    }
    // The same phase said again is not a boundary.
    status.set_phase("phase probe two");
    status.set_phase("phase probe three");

    let seen = reports
        .0
        .lock()
        .map(|seen| seen.clone())
        .unwrap_or_default();
    let [(phase, queries, top)] = seen.as_slice() else {
        panic!("one report for the one phase that ended: {seen:?}");
    };
    assert_eq!(phase, "phase probe two");
    assert_eq!(*queries, 35);
    assert!(top.starts_with("parse_probe=30 infer_probe=5"), "{top}");
}
