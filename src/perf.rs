//! Repeatable performance measurement: run records, their environment, and
//! where they are kept. See `docs/planning/performance-metrics.md`.

mod compare;
mod environment;
mod identity;
mod nextest;
mod profile;
mod record;
mod report;
mod spread;
mod store;
mod timing;
mod workflow;

pub use compare::{
    Comparison, CounterDelta, DEFAULT_TOLERANCE, Delta, Verdict, compare, judge, render_comparison,
};
pub use environment::{ChangeKind, EnvironmentChange, PerfEnvironment, RA_AP_VERSION};
pub use identity::{PerfIdentity, PerfIdentityBuilder};
pub use nextest::{NextestRequest, TestRun, parse_events};
pub use profile::{TestProfile, aggregate, attach, profiled, fixed_ns, parse_duration, parse_report, read_profiles};
pub use record::{
    FunctionTiming, PerfState, PerfSubject, RECORD_SCHEMA, RunRecord, StageTiming, SubjectKind,
    TestTiming,
};
pub use report::{format_nanos, render_profile, render_stages, render_timing};
pub use spread::Spread;
pub use store::PerfStore;
pub use timing::{TimingRequest, measure, measure_from, summarise};
pub use workflow::{StageSample, WorkflowRequest, WorkflowRequestBuilder, fold, measure_workflow};
