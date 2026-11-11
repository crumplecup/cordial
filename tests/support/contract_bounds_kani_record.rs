//! A Kani type record whose fragment is `value >= 0`.
#![cfg(feature = "antipatterns")]

use cordial::ContractRecordDump;

use crate::contract_bounds_dump::contract_record_dump;

pub fn kani_type_record(kind: &str, evidence: &str) -> miette::Result<ContractRecordDump> {
    contract_record_dump(evidence, "kani", kind, "value >= 0")
}
