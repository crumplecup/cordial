//! Leftover stdio macros in `src/` and `tests/`.
//!
//! **What.** Finds direct terminal/debug output that should be a tracing event
//! or a deliberate user-interface path.
//!
//! **Why.** Stdio macros bypass subscriber filtering and make observability
//! policy inconsistent. In a tracing-owned codebase, debug output should be
//! explicit and routable.
//!
//! **Flags.** `println!`, `eprintln!`, `print!`, `eprint!`, and `dbg!`.
//!
//! **Ignores.** Configured folders and cargo-protocol paths may be skipped.
//! `--apply` does not rewrite these rows.
//!
//! **Outputs.** `tracing-print.checklist.md`, summary, and CSV artifacts
//! through the parent `tracing` etiquette.
//!
//! **Config.** `[tracing.stdio]` owns macro filters, skipped folders, and
//! cargo-protocol policy. It is part of
//! [`crate::etiquettes::tracing::TRACING_ETIQUETTE`].

mod assessor;
mod enricher;
mod probe;
mod reporter;
mod scan;
mod types;

pub use assessor::PrintAssessor;
pub use enricher::PrintInventoryEnricher;
pub use probe::PrintSiteProbe;
pub use reporter::{PrintChecklistReporter, PrintCsvReporter, PrintSummaryReporter};
pub use scan::{scan_crate_tracing_print, scan_rust_source};
pub use types::{PrintRuleId, PrintSiteRecord};
