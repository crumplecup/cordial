//! Framework-std finding types: homecoming coverage plus gated amenable coverage.

#[cfg(feature = "amenable_std")]
mod amenable;
mod homecoming;

pub const HOMECOMING_STD_CATEGORY: &str = "homecoming-std";
pub const AMENABLE_STD_CATEGORY: &str = "amenable-std";

pub use homecoming::{
    FrameworkStdRowFinding, FrameworkStdRule, FrameworkStdScopeMarker,
    framework_gaps_from_findings, framework_report_from_findings, homecoming_row_disposition,
};

#[cfg(feature = "amenable_std")]
pub use amenable::{
    AmenableStdRowFinding, AmenableStdRule, amenable_gaps_from_findings,
    amenable_report_from_findings, amenable_row_disposition,
};
