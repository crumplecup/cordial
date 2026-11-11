//! A contract record as the registry dumps it.
#![cfg(feature = "antipatterns")]

use cordial::ContractRecordDump;
use miette::{IntoDiagnostic, WrapErr};

pub fn contract_record_dump(
    evidence: impl Into<String>,
    verifier: impl Into<String>,
    kind: impl Into<String>,
    fragment: impl Into<String>,
) -> miette::Result<ContractRecordDump> {
    ContractRecordDump::builder()
        .evidence(evidence.into())
        .verifier(verifier.into())
        .kind(kind.into())
        .fragment(fragment.into())
        .build()
        .into_diagnostic()
        .wrap_err("contract record dump")
}
