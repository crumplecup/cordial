//! Timing and persistence for cost-based gate tiers.

mod ledger;
mod partition;
mod table;
mod timing;

pub use ledger::{EtiquetteLedgerEntry, GateLedger};
pub use partition::{COLD_THRESHOLD, WARM_THRESHOLD, tier_for_timing};
pub use table::{BUILTIN_TIER_ASSIGNMENTS, builtin_tier};
#[cfg(feature = "rustdoc")]
pub(crate) use timing::is_timing_process;
#[cfg(any(
    feature = "rustdoc",
    feature = "doc_warnings",
    feature = "feature_warnings"
))]
pub(crate) use timing::timing_target_dir;
pub use timing::{HookTiming, HookTimings};
