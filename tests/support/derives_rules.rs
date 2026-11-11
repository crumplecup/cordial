//! The rule ids a Rust source snippet raises under the derives lint.

use cordial::{DeriveRuleId, DerivesThresholds};

use crate::derives_scan::scan_findings;

pub fn scan_rules(
    source: &str,
    thresholds: DerivesThresholds,
) -> miette::Result<Vec<DeriveRuleId>> {
    Ok(scan_findings(source, thresholds)?
        .into_iter()
        .map(|record| record.rule_id())
        .collect())
}
