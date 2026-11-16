//! Count the queries Salsa executes, by name and by the step of cordial that asked.

/// How many times Salsa executed each query, by name, from its own events.
#[derive(Default, Clone)]
pub struct Executions(std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, usize>>>);

impl<S> tracing_subscriber::Layer<S> for Executions
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if event.metadata().target() != "salsa::function::execute" {
            return;
        }
        struct Message(String);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        let query = message.0.split('(').next().unwrap_or_default();
        // The innermost span of cordial's own that the query runs under says which step asked.
        let step = context
            .event_scope(event)
            .and_then(|scope| {
                scope
                    .into_iter()
                    .find(|span| span.metadata().target().starts_with("cordial"))
                    .map(|span| span.name())
            })
            .unwrap_or("outside cordial");
        let name = format!("{step} > {query}");
        if let Ok(mut counts) = self.0.lock() {
            *counts.entry(name).or_default() += 1;
        }
    }
}

impl Executions {
    pub fn take(&self) -> std::collections::BTreeMap<String, usize> {
        self.0
            .lock()
            .map(|mut counts| std::mem::take(&mut *counts))
            .unwrap_or_default()
    }
}
