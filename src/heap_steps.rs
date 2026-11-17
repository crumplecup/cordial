//! Memory told by the step of the program that made it.
//!
//! The allocator tags each allocation with the query executing when it was made, and what is made
//! outside a query (the loader's texts, the graph a projection builds, a decoded state) is held by
//! no query. This layer makes the spans of the steps that make it the tag instead, while they are
//! entered, so that `cordial::heap` events and the memory held by each tell it by step, and a
//! free is told by the step that did it.

use std::cell::RefCell;

use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

thread_local! {
    /// The tag each entered span replaced, innermost last.
    static REPLACED: RefCell<Vec<Option<u32>>> = const { RefCell::new(Vec::new()) };
}

/// Tags the allocations made while a span is entered with the span's name.
#[derive(Debug, Clone, Copy, Default)]
pub struct HeapStepLayer;

impl<S> Layer<S> for HeapStepLayer
where
    S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_enter(&self, id: &tracing::span::Id, context: Context<'_, S>) {
        let replaced = context
            .span(id)
            .and_then(|span| salsa::enter_heap_step(span.name()));
        let _ = REPLACED.try_with(|stack| stack.borrow_mut().push(replaced));
    }

    fn on_exit(&self, _id: &tracing::span::Id, _context: Context<'_, S>) {
        let replaced = REPLACED
            .try_with(|stack| stack.borrow_mut().pop())
            .ok()
            .flatten()
            .flatten();
        if let Some(tag) = replaced {
            salsa::leave_heap_step(tag);
        }
    }
}

/// The spans that are steps of a run: the loading and saving of the database, the priming and the
/// projections, and not the queries (which the allocator already tells by ingredient).
#[tracing::instrument(level = "trace")]
pub fn heap_step_filter() -> tracing_subscriber::filter::Targets {
    tracing_subscriber::filter::Targets::new()
        .with_target(
            "cordial::loader::database::projection",
            tracing::Level::DEBUG,
        )
        .with_target("cordial::loader::database::prime", tracing::Level::DEBUG)
        .with_target("cordial::loader::database::saved", tracing::Level::DEBUG)
        .with_target("cordial::loader::database::session", tracing::Level::DEBUG)
        .with_target("cordial::loader::database::contexts", tracing::Level::DEBUG)
        .with_target("cordial::loader::database::populate", tracing::Level::DEBUG)
        .with_target("cordial::loader::source", tracing::Level::DEBUG)
        .with_target("cordial::session", tracing::Level::DEBUG)
}
