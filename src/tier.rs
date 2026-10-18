//! Timing and persistence for cost-based gate tiers.

mod ledger;
mod timing;

pub use ledger::{EtiquetteLedgerEntry, GateLedger};
#[cfg(feature = "rustdoc")]
pub(crate) use timing::is_timing_process;
#[cfg(any(
    feature = "rustdoc",
    feature = "doc_warnings",
    feature = "feature_warnings"
))]
pub(crate) use timing::timing_target_dir;
pub use timing::{HookTiming, HookTimings};
