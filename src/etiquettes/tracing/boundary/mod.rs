//! Binary error-boundary policy for tracing.
//!
//! **What.** Checks whether fallible binary entry points report their error
//! through tracing before the process boundary.
//!
//! **Why.** Library code should keep propagating errors with `?`, but a
//! binary that lets the final error bubble out has crossed the last useful
//! observability boundary.
//!
//! **Flags.** A fallible `fn main` that never emits a `tracing::warn!` or
//! `tracing::error!` event for the returned error.
//!
//! **Ignores.** Library functions and ordinary propagation sites belong to
//! the error-chain policy. This sub-etiquette applies only to the binary's own
//! entry point.
//!
//! **Outputs.** `tracing-boundary.checklist.md`, summary, and CSV artifacts
//! through the parent `tracing` etiquette.
//!
//! **Config.** `[tracing.boundary]` controls this policy. It is part of
//! [`crate::etiquettes::tracing::TRACING_ETIQUETTE`].

mod assessor;
mod detect;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::BoundaryAssessor;
pub use enricher::BoundaryInventoryEnricher;
pub use probe::BoundarySiteProbe;
pub use reporter::{BoundaryChecklistReporter, BoundaryCsvReporter, BoundarySummaryReporter};
pub use scan::scan_crate_tracing_boundary;
pub use types::{BoundaryRuleId, BoundarySiteRecord};
