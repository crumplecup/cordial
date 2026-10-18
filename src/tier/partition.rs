//! Cost thresholds chosen from the 2026-10-17 Cordial and Amenable measurements.

use std::time::Duration;

use tracing::instrument;

/// Cold passes above this gap pay for a build or substantial first-run setup.
pub const COLD_THRESHOLD: Duration = Duration::from_secs(20);
/// Warm passes above this gap are too costly for the default repeatable gate.
pub const WARM_THRESHOLD: Duration = Duration::from_secs(8);

/// Assign a measured etiquette to a cost tier, checking warm cost first.
#[instrument(level = "trace")]
pub fn tier_for_timing(cold: Duration, warm: Duration) -> &'static str {
    if warm > WARM_THRESHOLD {
        "deep"
    } else if cold > COLD_THRESHOLD {
        "build"
    } else {
        "quick"
    }
}
