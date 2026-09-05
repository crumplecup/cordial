//! Tracing-subscriber initialization policy.
//!
//! **What.** Checks whether crates have a documented, reusable tracing
//! subscriber initialization path.
//!
//! **Why.** Instrumentation is only useful when binaries and tests install a
//! subscriber consistently. A shared helper keeps initialization idempotent and
//! avoids one-off setup drift.
//!
//! **Flags.** Missing binary-main initialization, missing test
//! initialization, missing library subscriber story, `RUST_LOG` /
//! `EnvFilter` policy mismatches, and non-idempotent initialization.
//!
//! **Ignores.** This sub-etiquette does not decide per-function span recipes;
//! that belongs to the parent tracing instrument checks.
//!
//! **Outputs.** `tracing-subscriber.checklist.md`, summary, and CSV artifacts
//! through the parent `tracing` etiquette.
//!
//! **Config.** `[tracing.subscriber]` controls this policy. It is part of
//! [`crate::etiquettes::tracing::TRACING_ETIQUETTE`].

mod assessor;
mod detect;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::SubscriberAssessor;
pub use enricher::SubscriberInventoryEnricher;
pub use probe::SubscriberSiteProbe;
pub use reporter::{SubscriberChecklistReporter, SubscriberCsvReporter, SubscriberSummaryReporter};
pub use scan::scan_crate_tracing_subscriber;
pub use types::{SubscriberRuleId, SubscriberSiteRecord};
