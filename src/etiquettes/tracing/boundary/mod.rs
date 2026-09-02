//! Binary error-boundary policy: a fallible `fn main` must convert its
//! error to a tracing warn/error emission before the process boundary
//! instead of letting it bubble up and crash the process. Library code
//! keeps propagating errors via `?` (the existing error-chain policy);
//! this etiquette applies only to the binary's own entry point, and only
//! `stdio`-locked-down projects have a single designated UI channel
//! (`tracing::warn!`/`error!`) to check for.

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
