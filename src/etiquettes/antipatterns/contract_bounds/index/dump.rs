//! Registry dump records used by the contract-bound scanner.

use serde::{Deserialize, Serialize};
use tracing::instrument;

/// One registered `amenable_core::Ensures`/`Requires` contract fragment.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    derive_builder::Builder,
    derive_getters::Getters,
)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ContractRecordDump {
    /// Supporting evidence paths or labels.
    evidence: String,
    /// Proof verifier this row is about (`kani`, `creusot`, ...).
    verifier: String,
    /// Contract kind (`ensures`, `requires`, ...).
    kind: String,
    /// Source fragment of the contract bound.
    fragment: String,
}

impl ContractRecordDump {
    /// Start a builder for this value.
    #[instrument(level = "debug")]
    pub fn builder() -> ContractRecordDumpBuilder {
        ContractRecordDumpBuilder::default()
    }
}

#[derive(Debug, Clone, Default, Deserialize, derive_getters::Getters)]
pub(in crate::etiquettes::antipatterns::contract_bounds) struct RegistryDump {
    #[serde(default)]
    contract_records: Vec<ContractRecordDump>,
}

impl RegistryDump {
    #[instrument(level = "trace", skip(self))]
    pub(in crate::etiquettes::antipatterns::contract_bounds) fn into_contract_records(
        self,
    ) -> Vec<ContractRecordDump> {
        self.contract_records
    }
}
