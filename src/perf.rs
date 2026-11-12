//! Repeatable performance measurement: run records, their environment, and
//! where they are kept. See `docs/planning/performance-metrics.md`.

mod environment;
mod identity;
mod record;
mod spread;
mod store;

pub use environment::{ChangeKind, EnvironmentChange, PerfEnvironment, RA_AP_VERSION};
pub use identity::{PerfIdentity, PerfIdentityBuilder};
pub use record::{
    FunctionTiming, PerfState, PerfSubject, RECORD_SCHEMA, RunRecord, StageTiming, SubjectKind,
    TestTiming,
};
pub use spread::Spread;
pub use store::PerfStore;
