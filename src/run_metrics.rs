//! What a run says about itself, as numbers, kept for the performance record.
//!
//! The events that say how a run went (queries executed and verified per phase, what the audit
//! found, what fell back to rust-analyzer, how much of the saved state was read and how fast, what
//! flattening the saved memos reached) carry their numbers as fields. This layer adds those fields
//! up as the events happen, whatever `RUST_LOG` says, and the run writes them next to its ledger, so
//! that `cordial perf workflow` can put them in the record it makes. Nothing is measured twice: a
//! number here is a field of an event a person reading the log sees too.
//!
//! A name is the event's target, then what it says it is about (the `ingredient`, `phase`, `step`,
//! `part` or `what` field, if it has one), then the field. Numbers are summed, and the number of
//! events is `events`. Text fields that are worth reading but cannot be added up (which kinds of
//! query reached the most, why a fingerprint was volatile) are kept as notes, the last one seen.

use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use tracing::instrument;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::{Context, Layer};

use crate::{CordialResult, StoreLayout};

/// Where a run's metrics are written, in its store.
const FILE: &str = "metrics.json";

/// The most names a run keeps, so that a field with many values cannot grow the file without end.
const LIMIT: usize = 4_000;

/// The fields that say what an event is about, in the order they are looked for.
const QUALIFIERS: &[&str] = &["ingredient", "phase", "step", "part", "what"];

/// The text fields kept as notes.
const NOTES: &[&str] = &["breadth", "reads_themselves", "kinds", "top", "reasons"];

/// What a run said about itself.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct RunMetrics {
    /// Each number, summed over the events that carried it.
    #[serde(default)]
    values: BTreeMap<String, u64>,
    /// Text that cannot be added up, the last of each.
    #[serde(default)]
    notes: BTreeMap<String, String>,
}

static CURRENT: LazyLock<Mutex<RunMetrics>> = LazyLock::new(|| Mutex::new(RunMetrics::default()));

/// The events [`MetricsLayer`] reads, and the level it reads them at.
#[instrument(level = "trace")]
pub fn metrics_filter() -> Targets {
    Targets::new()
        .with_target("cordial::phase_queries", tracing::Level::INFO)
        .with_target("cordial::audit", tracing::Level::INFO)
        .with_target("cordial::semantic::fallback", tracing::Level::INFO)
        .with_target("cordial::cache", tracing::Level::DEBUG)
        .with_target("salsa::persistence", tracing::Level::DEBUG)
        .with_target("salsa::database", tracing::Level::DEBUG)
}

/// Adds up the numeric fields of the events [`metrics_filter`] lets through.
#[derive(Debug, Clone, Copy, Default)]
pub struct MetricsLayer;

#[derive(Default)]
struct Fields {
    numbers: Vec<(&'static str, u64)>,
    qualifier: Option<String>,
    notes: Vec<(&'static str, String)>,
}

impl Fields {
    fn text(&mut self, field: &tracing::field::Field, value: String) {
        let name = field.name();
        if self.qualifier.is_none() && QUALIFIERS.contains(&name) {
            self.qualifier = Some(slug(&value));
        } else if let Some(note) = NOTES.iter().find(|note| **note == name) {
            self.notes.push((note, value));
        }
    }
}

impl tracing::field::Visit for Fields {
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.numbers.push((field.name(), value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        if let Ok(value) = u64::try_from(value) {
            self.numbers.push((field.name(), value));
        }
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.numbers.push((field.name(), u64::from(value)));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.text(field, value.to_owned());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() != "message" {
            self.text(field, format!("{value:?}").trim_matches('"').to_owned());
        }
    }
}

/// `text` as a name: lower case words joined with `_`, digits dropped, so that "Analysing cfg
/// context 7" and "Analysing cfg context 8" are one thing.
fn slug(text: &str) -> String {
    text.split(|c: char| !c.is_ascii_alphabetic())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("_")
}

/// The name of an event's target as it is in a metric: without the crate it is in.
fn prefix(target: &str) -> String {
    target
        .strip_prefix("cordial::")
        .or_else(|| target.strip_prefix("salsa::"))
        .unwrap_or(target)
        .replace("::", ".")
}

impl<S: tracing::Subscriber> Layer<S> for MetricsLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let mut name = prefix(event.metadata().target());
        if let Some(qualifier) = fields.qualifier.filter(|qualifier| !qualifier.is_empty()) {
            name = format!("{name}.{qualifier}");
        }
        let Ok(mut current) = CURRENT.lock() else {
            return;
        };
        for (field, value) in fields.numbers {
            add(&mut current, format!("{name}.{field}"), value);
        }
        add(&mut current, format!("{name}.events"), 1);
        for (field, text) in fields.notes {
            if current.notes.len() < LIMIT || current.notes.contains_key(&format!("{name}.{field}"))
            {
                current.notes.insert(format!("{name}.{field}"), text);
            }
        }
    }
}

fn add(current: &mut RunMetrics, name: String, value: u64) {
    if current.values.len() >= LIMIT && !current.values.contains_key(&name) {
        return;
    }
    let total = current.values.entry(name).or_default();
    *total = total.saturating_add(value);
}

/// Write what the run has said, with what the process knows of itself, to the store.
#[instrument(level = "debug", skip(store), err(level = "warn"))]
#[hotpath::measure]
pub fn write(store: &StoreLayout) -> CordialResult<()> {
    let mut metrics = CURRENT
        .lock()
        .map(|current| current.clone())
        .unwrap_or_default();
    metrics.values.insert(
        "run.queries_executed".to_owned(),
        crate::LiveStatus::global().executed(),
    );
    if let Some(memory) = crate::process_memory::sample() {
        metrics.values.insert(
            "run.peak_memory.bytes".to_owned(),
            memory.peak_kib().saturating_mul(1024),
        );
    }
    std::fs::create_dir_all(store.root())?;
    std::fs::write(
        store.root().join(FILE),
        serde_json::to_vec_pretty(&metrics)?,
    )?;
    Ok(())
}

/// What the run that wrote `store`'s metrics said, or nothing when it wrote none.
#[instrument(level = "debug", skip(store))]
#[hotpath::measure]
pub fn read(store: &StoreLayout) -> RunMetrics {
    std::fs::read(store.root().join(FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}
