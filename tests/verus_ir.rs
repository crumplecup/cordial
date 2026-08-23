#![cfg(feature = "verus_ir")]

use std::path::Path;

use cordial::{VerusFnMode, VerusPanicKind, VerusPublish, scan_verus_rust_source};

/// Reproduces `cstr_carrier.rs::verify_cstr_excludes_the_terminating_nul_from_to_bytes`
/// -- the exact function whose real `@` view-operator usage broke the
/// syn-only best-effort recovery in `panics::verus_recover`. `verus_syn`
/// should parse this cleanly, since it's a real Verus-aware parser, not
/// a token-walking approximation.
const VIEW_OPERATOR_SOURCE: &str = r#"
use verus_builtin_macros::verus;

verus! {

pub open spec fn non_nul_byte_value_is_nonzero(byte: u8) -> bool {
    byte != 0
}

pub fn verify_cstr_excludes_the_terminating_nul_from_to_bytes(byte: u8) -> (result: bool)
    requires
        non_nul_byte_value_is_nonzero(byte),
    ensures
        result,
{
    let with_nul: &[u8] = &[byte, 0];
    let cstr_result = CStr::from_bytes_with_nul(with_nul);
    assert(cstr_result is Ok);
    let cstr = cstr_result.unwrap();

    let bytes = cstr.to_bytes();
    assert(bytes@.len() == 1);
    assert(bytes@[0] == byte);
    true
}

}
"#;

#[test]
fn parses_a_function_whose_body_uses_the_view_operator() {
    let ir = scan_verus_rust_source(
        VIEW_OPERATOR_SOURCE,
        Path::new("cstr_carrier.rs"),
        "rust_std::cstr_carrier",
    );

    let names: Vec<&str> = ir.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(
        names.contains(&"verify_cstr_excludes_the_terminating_nul_from_to_bytes"),
        "{names:?}"
    );
    assert!(names.contains(&"non_nul_byte_value_is_nonzero"), "{names:?}");

    let spec_fn = ir
        .functions
        .iter()
        .find(|f| f.name == "non_nul_byte_value_is_nonzero")
        .expect("spec fn present");
    assert_eq!(spec_fn.mode, VerusFnMode::Spec);
    assert_eq!(spec_fn.publish, VerusPublish::Open);

    let exec_fn = ir
        .functions
        .iter()
        .find(|f| f.name == "verify_cstr_excludes_the_terminating_nul_from_to_bytes")
        .expect("exec fn present");
    assert_eq!(exec_fn.requires, vec!["non_nul_byte_value_is_nonzero (byte)"]);
    assert!(!exec_fn.uses_assume);
    assert!(!exec_fn.uses_admit);
    assert!(!exec_fn.is_external_body);

    // The real completion of panics::verus_recover's own motivating gap:
    // this exact function's real .unwrap() call (invisible to that
    // best-effort recovery because a *later* line in the same body uses
    // the `@` view operator, failing the whole block's syn::Block parse)
    // is found here via a real, complete parse.
    assert_eq!(exec_fn.panic_sites.len(), 1, "{:?}", exec_fn.panic_sites);
    assert_eq!(exec_fn.panic_sites[0].kind, VerusPanicKind::Unwrap);
}

const ASSUME_AND_AXIOM_SOURCE: &str = r#"
use verus_builtin_macros::verus;

verus! {

axiom fn axiom_addition_commutes(a: int, b: int)
    ensures
        a + b == b + a,
{
}

proof fn trusts_a_local_claim(x: int)
    ensures
        x == x,
{
    assume(x == x);
}

#[verifier::external_body]
fn opts_out_of_verification() -> (result: bool)
    ensures
        result,
{
    true
}

fn calls_admit_directly()
{
    admit();
}

}
"#;

#[test]
fn detects_real_soundness_escape_hatches() {
    let ir = scan_verus_rust_source(
        ASSUME_AND_AXIOM_SOURCE,
        Path::new("soundness_sample.rs"),
        "gallery::soundness_sample",
    );

    let find = |name: &str| {
        ir.functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name} not found in {:?}", ir.functions))
    };

    let axiom = find("axiom_addition_commutes");
    assert_eq!(axiom.mode, VerusFnMode::ProofAxiom);
    assert!(axiom.is_trusted_not_proven());

    let assumes = find("trusts_a_local_claim");
    assert!(assumes.uses_assume);
    assert!(assumes.is_trusted_not_proven());

    let external = find("opts_out_of_verification");
    assert!(external.is_external_body);
    assert!(external.is_trusted_not_proven());

    let admits = find("calls_admit_directly");
    assert!(admits.uses_admit);
    assert!(admits.is_trusted_not_proven());

    assert_eq!(
        ir.trusted_not_proven().count(),
        4,
        "expected every one of the 4 real escape hatches to be flagged: {:?}",
        ir.functions
    );
}

const SIGNATURE_FACTS_SOURCE: &str = r#"
use verus_builtin_macros::verus;

verus! {

pub broadcast proof fn lemma_applies_everywhere(tracked cred: Cred)
    recommends
        cred.is_valid(),
    ensures
        true,
{
}

pub fn matches_on_result(x: i32) -> (result: bool)
{
    match x {
        0 => panic!("zero"),
        1 => unreachable!(),
        _ => x.checked_div(2).expect("nonzero"),
    };
    x.checked_div(2).unwrap_err().is_some()
}

}
"#;

#[test]
fn extracts_signature_level_facts_and_every_panic_site_kind() {
    let ir = scan_verus_rust_source(
        SIGNATURE_FACTS_SOURCE,
        Path::new("signature_sample.rs"),
        "gallery::signature_sample",
    );

    let find = |name: &str| {
        ir.functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name} not found in {:?}", ir.functions))
    };

    let lemma = find("lemma_applies_everywhere");
    assert!(lemma.is_broadcast);
    assert_eq!(lemma.tracked_params, vec!["cred"]);
    assert_eq!(lemma.recommends, vec!["cred . is_valid ()"]);

    let matcher = find("matches_on_result");
    let kinds: Vec<VerusPanicKind> = matcher.panic_sites.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        vec![
            VerusPanicKind::Panic,
            VerusPanicKind::Unreachable,
            VerusPanicKind::Expect,
            VerusPanicKind::Unwrap,
        ],
        "{:?}",
        matcher.panic_sites
    );
}

const CFG_TEST_AND_COMPILE_ERROR_SOURCE: &str = r#"
use verus_builtin_macros::verus;

verus! {

pub fn in_library_code() -> (result: bool)
{
    compile_error!("should never reach exec");
    true
}

}

#[cfg(test)]
mod tests {
    use verus_builtin_macros::verus;

    verus! {

    pub fn in_test_code() -> (result: bool)
    {
        true
    }

    }
}
"#;

#[test]
fn tracks_cfg_test_module_nesting_and_detects_compile_error() {
    let ir = scan_verus_rust_source(
        CFG_TEST_AND_COMPILE_ERROR_SOURCE,
        Path::new("cfg_test_sample.rs"),
        "gallery::cfg_test_sample",
    );

    let find = |name: &str| {
        ir.functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name} not found in {:?}", ir.functions))
    };

    let library_fn = find("in_library_code");
    assert!(!library_fn.cfg_test);
    assert_eq!(
        library_fn.panic_sites[0].kind,
        VerusPanicKind::CompileError,
        "{:?}",
        library_fn.panic_sites
    );

    let test_fn = find("in_test_code");
    assert!(test_fn.cfg_test);
}
