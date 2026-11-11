//! Shape matrix, Verus and Creusot: bare results, named calls, builtin items, and trigger equations.
#![cfg(feature = "antipatterns")]

use cordial::{
    ContractRecordDump, scan_creusot_contract_bounds_source, scan_verus_contract_bounds_source,
};

#[path = "support/contract_bounds_dump.rs"]
mod contract_bounds_dump;

#[path = "support/contract_bounds_logic_record.rs"]
mod contract_bounds_logic_record;

#[path = "support/contract_bounds_shape_case.rs"]
mod contract_bounds_shape_case;

use contract_bounds_dump::contract_record_dump;
use contract_bounds_logic_record::logic_fn_record;
use contract_bounds_shape_case::{ShapeCase, check_cases};

fn empty_registry() -> miette::Result<Vec<ContractRecordDump>> {
    Ok(Vec::new())
}

fn write_stores_new_value_registry() -> miette::Result<Vec<ContractRecordDump>> {
    Ok(vec![logic_fn_record(
        "verus",
        "ensures",
        "write_stores_new_value",
    )?])
}

fn value_to_owned_identity_registry() -> miette::Result<Vec<ContractRecordDump>> {
    Ok(vec![contract_record_dump(
        "fixture::value_to_owned_is_identity",
        "verus",
        "ensures",
        "pub open spec fn value_to_owned_is_identity(value: i32) -> bool { to_owned_spec(value) == value }",
    )?])
}

fn into_i32_identity_registry() -> miette::Result<Vec<ContractRecordDump>> {
    Ok(vec![contract_record_dump(
        "fixture::into_i32_spec_matches_input",
        "verus",
        "ensures",
        "pub open spec fn into_i32_spec_matches_input(v: i32) -> bool { into_i32_spec(v) == v }",
    )?])
}

fn i32_and_bool_type_ids_differ_registry() -> miette::Result<Vec<ContractRecordDump>> {
    Ok(vec![contract_record_dump(
        "fixture::i32_and_bool_type_ids_differ",
        "verus",
        "ensures",
        // Deliberately spaced turbofish (`:: <`, not `::<`) -- matches
        // how `verus_ensures_predicate!`'s real codegen actually
        // renders a fragment (re-emitted via a fresh `TokenStream`,
        // which picks Alone spacing for a constructed `::` + `<` pair)
        // rather than the compact `::<` a hand-typed literal or a
        // clause parsed straight from real source produces. Written
        // this way on purpose so this fixture actually exercises the
        // Joint/Alone mismatch `is_raw_duplicate_of_named_sibling`'s
        // `canonicalize_type_text` comparison exists to paper over --
        // a compact `::<` fragment string would parse back with the
        // same spacing as the clause and pass even without that fix.
        "open spec fn i32_and_bool_type_ids_differ() -> bool { type_id_spec :: < i32 > () != type_id_spec :: < bool > () }",
    )?])
}

const SHAPE_CASES: &[ShapeCase] = &[
    ShapeCase {
        id: "creusot_trivial_bare_result",
        scan: scan_creusot_contract_bounds_source,
        kind: "ensures",
        source: r#"
amenable_derive::harness! {
    creusot, VERIFY_SOMETHING_SRC, {
        #[trusted]
        #[ensures(result)]
        fn verify_something() -> bool {
            match 1 {
                1 => true,
                _ => false,
            }
        }
    }
}
"#,
        registry: empty_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "verus_trivial_bare_result",
        scan: scan_verus_contract_bounds_source,
        kind: "ensures",
        source: r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub fn verify_something() -> (result: bool)
    ensures
        result,
{
    true
}

} // verus!
"#,
        registry: empty_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "verus_named_call_with_final_argument",
        scan: scan_verus_contract_bounds_source,
        kind: "ensures",
        source: r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

pub struct VerusCellModel {
    value: i32,
}

verus! {

impl VerusCellModel {
    pub fn set(&mut self, new_value: i32)
        ensures
            write_stores_new_value(new_value as int, final(self).value as int),
    {
        self.value = new_value;
    }
}

} // verus!
"#,
        registry: write_stores_new_value_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "verus_negated_bare_result",
        scan: scan_verus_contract_bounds_source,
        kind: "ensures",
        source: r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub fn verify_something() -> (result: bool)
    ensures
        !result,
{
    false
}

} // verus!
"#,
        registry: empty_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "verus_builtin_function_item_contract_inspection",
        scan: scan_verus_contract_bounds_source,
        kind: "ensures",
        source: r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub assume_specification<H: core::default::Default> [<H as core::default::Default>::default] () -> (result: H)
    ensures
        H::default.ensures((), result),
;

} // verus!
"#,
        // Registry-independent: `H::default.ensures((), result)` is
        // Verus's own builtin function-item contract-inspection syntax
        // (`SomeFn.ensures(args)`/`SomeFn.requires(args)`, spelled
        // identically to the real clause-list keyword but a genuine
        // method call, inspecting an *external* function's own contract)
        // -- there is no local `fn` this could ever be pointed at, so an
        // empty registry still must not flag it. Found via real
        // dogfooding against `amenable_verus::rust_std::collections::
        // hash_carrier`'s own identical site.
        registry: empty_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "verus_raw_trigger_equation_restating_named_sibling",
        scan: scan_verus_contract_bounds_source,
        kind: "ensures",
        source: r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub uninterp spec fn to_owned_spec(value: i32) -> i32;

pub open spec fn value_to_owned_is_identity(value: i32) -> bool {
    to_owned_spec(value) == value
}

#[verifier::external_body]
pub broadcast proof fn axiom_value_to_owned_is_identity(value: i32)
    ensures
        #[trigger] to_owned_spec(value) == value,
        value_to_owned_is_identity(value),
{
}

} // verus!
"#,
        // A raw `#[trigger]`ed equation sitting right next to a bare
        // (un-decorated) named call restating the identical claim --
        // Verus's own automatic broadcast/trigger instantiation needs the
        // literal equation present as its own `#[trigger]`ed clause (a
        // call wrapping the same equation inside a named predicate gives
        // the solver nothing to pattern-match on), so this project's
        // convention states the claim twice: once raw for the solver,
        // once named for the reader and the registry. Real production
        // instance: `cow_carrier.rs`'s `axiom_i32_to_owned_is_identity`,
        // already doc-commented in its own source explaining exactly
        // this. The registry below carries only the named sibling's own
        // real fragment source (matching how it's actually generated) --
        // the raw clause is silenced by finding it, not by a registry
        // entry of its own.
        registry: value_to_owned_identity_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "verus_raw_trigger_equation_restating_a_trigger_decorated_named_sibling",
        scan: scan_verus_contract_bounds_source,
        kind: "ensures",
        source: r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub uninterp spec fn into_i32_spec(v: i32) -> i32;

pub open spec fn into_i32_spec_matches_input(v: i32) -> bool {
    into_i32_spec(v) == v
}

#[verifier::external_body]
pub broadcast proof fn axiom_i32_into_i32_is_identity(v: i32)
    ensures
        #[trigger] into_i32_spec(v) == v,
        #[trigger] into_i32_spec_matches_input(v),
{
}

} // verus!
"#,
        // Same restatement idiom as
        // `verus_raw_trigger_equation_restating_named_sibling` above, but
        // with the *named* sibling clause also `#[trigger]`ed -- real
        // production instance: `cstring_carrier.rs`'s
        // `axiom_vec_u8_into_vec_u8_is_identity`, where both ensures
        // clauses carry `#[trigger]`. A leading outer attribute on an
        // expression is real Rust grammar `bare_named_call_name`'s
        // pure-token scan doesn't special-case, so this needs the
        // `syn::Expr` fallback (`named_call_name_allowing_leading_attr`)
        // to recognize the sibling as a named call at all.
        registry: into_i32_identity_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "verus_raw_trigger_equation_with_two_trigger_attrs_restating_named_sibling",
        scan: scan_verus_contract_bounds_source,
        kind: "ensures",
        source: r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub uninterp spec fn type_id_spec<T: 'static + ?Sized>() -> nat;

pub open spec fn i32_and_bool_type_ids_differ() -> bool {
    type_id_spec::<i32>() != type_id_spec::<bool>()
}

#[verifier::external_body]
pub broadcast proof fn axiom_i32_and_bool_type_ids_differ()
    ensures
        #[trigger] type_id_spec::<i32>() != #[trigger] type_id_spec::<bool>(),
        i32_and_bool_type_ids_differ(),
{
}

} // verus!
"#,
        // A single clause carrying *two* `#[trigger]` attributes, one on
        // each side of the comparison -- real production instance:
        // `type_id_carrier.rs`'s `axiom_i32_and_bool_type_ids_differ`.
        // `strip_trigger_attrs` has to remove every one, not just a
        // leading one, before comparing against the named sibling's
        // body (which carries none at all).
        registry: i32_and_bool_type_ids_differ_registry,
        expect_flagged: false,
    },
];

#[test]
fn shape_matrix_matches_expected_flags() -> miette::Result<()> {
    cordial::init_tracing();
    check_cases(SHAPE_CASES)
}
