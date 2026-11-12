//! Repeatable performance measurement: run records, their environment, and
//! where they are kept. See `docs/planning/performance-metrics.md`.

mod environment;
mod identity;
mod nextest;
mod record;
mod report;
mod spread;
mod store;
mod timing;

pub use environment::{ChangeKind, EnvironmentChange, PerfEnvironment, RA_AP_VERSION};
pub use identity::{PerfIdentity, PerfIdentityBuilder};
pub use nextest::{NextestRequest, TestRun, parse_events};
pub use record::{
    FunctionTiming, PerfState, PerfSubject, RECORD_SCHEMA, RunRecord, StageTiming, SubjectKind,
    TestTiming,
};
pub use report::{format_nanos, render_timing};
pub use spread::Spread;
pub use store::PerfStore;
pub use timing::{TimingRequest, measure, measure_from, summarise};
