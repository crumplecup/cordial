//! Observe query execution through tracing, never an analysis-fact cache.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing_subscriber::{Layer, layer::Context};

#[derive(Clone, Default)]
pub struct QueryMetrics(Arc<Mutex<BTreeMap<String, usize>>>);

#[derive(Default)]
struct QueryEvent {
    function: String,
    query: String,
}

impl Visit for QueryEvent {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_str(field, &format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "function" => self.function = value.to_owned(),
            "query" => self.query = value.to_owned(),
            _ => {}
        }
    }
}

impl<S: tracing::Subscriber> Layer<S> for QueryMetrics {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        if event.metadata().target() != "cordial::semantic::queries" {
            return;
        }
        let mut fields = QueryEvent::default();
        event.record(&mut fields);
        let mut counts = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *counts
            .entry(format!("{}:{}", fields.query, fields.function))
            .or_default() += 1;
    }
}

impl QueryMetrics {
    pub fn snapshot(&self) -> BTreeMap<String, usize> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}
