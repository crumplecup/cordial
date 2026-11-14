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
