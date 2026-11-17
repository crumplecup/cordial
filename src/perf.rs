//! Repeatable performance measurement: run records, their environment, and
//! where they are kept. See `docs/planning/performance-metrics.md`.

mod compare;
mod environment;
mod identity;
mod nextest;
mod profile;
mod record;
mod report;
pub mod runlog;
mod spread;
mod store;
mod timing;
mod usage;
mod workflow;

pub use compare::{
    Comparison, CounterDelta, DEFAULT_TOLERANCE, Delta, Verdict, compare, judge, render_comparison,
};
pub use environment::{ChangeKind, EnvironmentChange, PerfEnvironment, RA_AP_VERSION};
pub use identity::{PerfIdentity, PerfIdentityBuilder};
pub use nextest::{NextestRequest, TestRun, build_targets, parse_events};
pub use profile::{
    TestProfile, aggregate, attach, fixed_ns, parse_duration, parse_report, profiled, read_profiles,
};
pub use record::{
    FunctionTiming, PerfState, PerfSubject, RECORD_SCHEMA, RunRecord, RunRecordBuilder,
    StageTiming, SubjectKind, TestTiming,
};
pub use report::{
    format_bytes, format_nanos, render_profile, render_stages, render_timing, render_usage,
};
pub use spread::Spread;
pub use store::PerfStore;
pub use timing::{TimingRequest, measure, measure_from, summarise};
pub use usage::{
    ChannelUsage, FunctionAllocation, LockUsage, ProfileUsage, ThreadUsage, counts_exactly,
    parse_bytes,
};
pub use workflow::{
    StageSample, WorkflowRequest, WorkflowRequestBuilder, fold, measure_workflow,
    measure_workflow_partial,
};
