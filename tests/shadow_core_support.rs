#![cfg(feature = "elicitation")]

use cordial::ShadowCoreSupportStatus;
use cordial::testing::{ImplCrateRollup, build_shadow_core_support_summary};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn impl_report_produces_core_tracked_summary() -> miette::Result<()> {
    let rollup = ImplCrateRollup {
        types: 1,
        our_traits_done: 1,
        direct_elicit_complete: 1,
        wrapper_covered_types: 0,
    };
    let summary =
        build_shadow_core_support_summary("url", "elicit_url", true, 1, true, Some(&rollup))
            .into_diagnostic()
            .wrap_err("summary")?;
    assert_eq!(summary.status, ShadowCoreSupportStatus::CoreTracked);
    assert_eq!(summary.our_traits_done, 1);
    Ok(())
}

#[test]
fn inventory_without_impl_report_is_core_pending() -> miette::Result<()> {
    let summary = build_shadow_core_support_summary("url", "elicit_url", true, 1, true, None)
        .into_diagnostic()
        .wrap_err("summary")?;
    assert_eq!(summary.status, ShadowCoreSupportStatus::CorePending);
    Ok(())
}
