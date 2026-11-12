//! Named test sets for `cordial perf tests`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One named set of tests, selected by a nextest filterset expression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct PerfTestSet {
    /// A nextest filterset such as `binary(verus_ir) | binary(panics_verus)`.
    filter: String,
}

/// Performance settings loaded from `[perf]` in `cordial.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct PerfConfig {
    /// Named test sets, from `[perf.tests.<name>]`.
    #[serde(default)]
    tests: BTreeMap<String, PerfTestSet>,
}
