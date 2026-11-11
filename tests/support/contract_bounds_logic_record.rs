//! A Creusot or Verus `#[logic(open)]` function record.
#![cfg(feature = "antipatterns")]

use cordial::ContractRecordDump;

use crate::contract_bounds_dump::contract_record_dump;

pub fn logic_fn_record(
    verifier: &str,
    kind: &str,
    fn_name: &str,
) -> miette::Result<ContractRecordDump> {
    contract_record_dump(
        format!("fixture::{fn_name}"),
        verifier,
        kind,
        format!("#[logic(open)]\nfn {fn_name}(c: char) -> bool {{\n    true\n}}"),
    )
}
