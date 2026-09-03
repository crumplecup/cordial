#![cfg(feature = "verus_ir")]

use std::path::Path;

use cordial::{
    SourceSpan, VerusCrateIr, VerusEnumFacts, VerusFnFacts, VerusFnMode, VerusPanicKind,
    VerusPublish, scan_verus_rust_source,
};

fn function_named<'a>(ir: &'a VerusCrateIr, name: &str) -> miette::Result<&'a VerusFnFacts> {
    ir.functions
        .iter()
        .find(|f| f.name == name)
        .ok_or_else(|| miette::miette!("{name} not found in {:?}", ir.functions))
}

fn enum_named<'a>(ir: &'a VerusCrateIr, name: &str) -> miette::Result<&'a VerusEnumFacts> {
    ir.enums
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| miette::miette!("{name} not found in {:?}", ir.enums))
}

/// A fully-documented enum with one data-carrying and one unit variant --
/// the real shape `TransferError`/the derived-witness result enums take
/// in `amenable_verus`.
const DOCUMENTED_DATA_CARRYING_ENUM_SOURCE: &str =
    include_str!("fixtures/verus_ir/documented_data_carrying_enum_source.rs");

/// Same shape, but the data-carrying variant has no doc comment -- a
/// real gap, not Verus's synthesized accessor.
const UNDOCUMENTED_VARIANT_ENUM_SOURCE: &str =
    include_str!("fixtures/verus_ir/undocumented_variant_enum_source.rs");

/// An all-unit-variant enum -- Verus never synthesizes a
/// pattern-projection accessor for one of these, so it's never exempt.
const UNIT_ONLY_ENUM_SOURCE: &str = include_str!("fixtures/verus_ir/unit_only_enum_source.rs");

/// Reproduces `cstr_carrier.rs::verify_cstr_excludes_the_terminating_nul_from_to_bytes`
/// -- the exact function whose real `@` view-operator usage broke the
/// syn-only best-effort recovery in `panics::verus_recover`. `verus_syn`
/// should parse this cleanly, since it's a real Verus-aware parser, not
/// a token-walking approximation.
const VIEW_OPERATOR_SOURCE: &str = include_str!("fixtures/verus_ir/view_operator_source.rs");

#[test]
fn parses_a_function_whose_body_uses_the_view_operator() -> miette::Result<()> {
    cordial::init_tracing();
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
    assert!(
        names.contains(&"non_nul_byte_value_is_nonzero"),
        "{names:?}"
    );

    let spec_fn = function_named(&ir, "non_nul_byte_value_is_nonzero")?;
    assert_eq!(spec_fn.mode, VerusFnMode::Spec);
    assert_eq!(spec_fn.publish, VerusPublish::Open);

    let exec_fn = function_named(
        &ir,
        "verify_cstr_excludes_the_terminating_nul_from_to_bytes",
    )?;
    assert_eq!(
        exec_fn.requires,
        vec!["non_nul_byte_value_is_nonzero (byte)"]
    );
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
    Ok(())
}

const ASSUME_AND_AXIOM_SOURCE: &str = include_str!("fixtures/verus_ir/assume_and_axiom_source.rs");

#[test]
fn detects_real_soundness_escape_hatches() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = scan_verus_rust_source(
        ASSUME_AND_AXIOM_SOURCE,
        Path::new("soundness_sample.rs"),
        "gallery::soundness_sample",
    );

    let axiom = function_named(&ir, "axiom_addition_commutes")?;
    assert_eq!(axiom.mode, VerusFnMode::ProofAxiom);
    assert!(axiom.is_trusted_not_proven());

    let assumes = function_named(&ir, "trusts_a_local_claim")?;
    assert!(assumes.uses_assume);
    assert!(assumes.is_trusted_not_proven());

    let external = function_named(&ir, "opts_out_of_verification")?;
    assert!(external.is_external_body);
    assert!(external.is_trusted_not_proven());

    let admits = function_named(&ir, "calls_admit_directly")?;
    assert!(admits.uses_admit);
    assert!(admits.is_trusted_not_proven());

    assert_eq!(
        ir.trusted_not_proven().count(),
        4,
        "expected every one of the 4 real escape hatches to be flagged: {:?}",
        ir.functions
    );
    Ok(())
}

const SIGNATURE_FACTS_SOURCE: &str = include_str!("fixtures/verus_ir/signature_facts_source.rs");

#[test]
fn extracts_signature_level_facts_and_every_panic_site_kind() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = scan_verus_rust_source(
        SIGNATURE_FACTS_SOURCE,
        Path::new("signature_sample.rs"),
        "gallery::signature_sample",
    );

    let lemma = function_named(&ir, "lemma_applies_everywhere")?;
    assert!(lemma.is_broadcast);
    assert_eq!(lemma.tracked_params, vec!["cred"]);
    assert_eq!(lemma.recommends, vec!["cred . is_valid ()"]);

    let matcher = function_named(&ir, "matches_on_result")?;
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
    Ok(())
}

const CFG_TEST_AND_COMPILE_ERROR_SOURCE: &str =
    include_str!("fixtures/verus_ir/cfg_test_and_compile_error_source.rs");

#[test]
fn tracks_cfg_test_module_nesting_and_detects_compile_error() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = scan_verus_rust_source(
        CFG_TEST_AND_COMPILE_ERROR_SOURCE,
        Path::new("cfg_test_sample.rs"),
        "gallery::cfg_test_sample",
    );

    let library_fn = function_named(&ir, "in_library_code")?;
    assert!(!library_fn.cfg_test);
    assert_eq!(
        library_fn.panic_sites[0].kind,
        VerusPanicKind::CompileError,
        "{:?}",
        library_fn.panic_sites
    );

    let test_fn = function_named(&ir, "in_test_code")?;
    assert!(test_fn.cfg_test);
    Ok(())
}

const GHOST_EXEC_UNREACHABLE_SOURCE: &str =
    include_str!("fixtures/verus_ir/ghost_exec_unreachable_source.rs");

#[test]
fn marks_only_the_unreachable_arm_with_a_real_ghost_sibling() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = scan_verus_rust_source(
        GHOST_EXEC_UNREACHABLE_SOURCE,
        Path::new("ghost_exec_sample.rs"),
        "gallery::ghost_exec_sample",
    );

    let paired = function_named(&ir, "matches_int_error_kind_carriers_own_shape")?;
    assert_eq!(paired.panic_sites.len(), 1, "{:?}", paired.panic_sites);
    assert!(
        paired.panic_sites[0].proven_unreachable_by_ghost_sibling,
        "{:?}",
        paired.panic_sites
    );

    let unpaired = function_named(&ir, "ordinary_unreachable_with_no_ghost_sibling")?;
    assert_eq!(unpaired.panic_sites.len(), 1, "{:?}", unpaired.panic_sites);
    assert!(
        !unpaired.panic_sites[0].proven_unreachable_by_ghost_sibling,
        "{:?}",
        unpaired.panic_sites
    );
    Ok(())
}

const LOCAL_CALL_SOURCE: &str = include_str!("fixtures/verus_ir/local_call_source.rs");

#[test]
fn records_local_call_target_names() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = scan_verus_rust_source(
        LOCAL_CALL_SOURCE,
        Path::new("local_call_sample.rs"),
        "gallery::local_call_sample",
    );

    let caller = function_named(&ir, "caller")?;
    assert!(
        caller.calls.contains(&"helper".to_string()),
        "{:?}",
        caller.calls
    );
    assert!(
        caller.calls.contains(&"from_str".to_string()),
        "{:?}",
        caller.calls
    );

    let helper = function_named(&ir, "helper")?;
    assert!(helper.calls.is_empty(), "{:?}", helper.calls);
    Ok(())
}

#[test]
fn fully_documented_data_carrying_enum_is_a_pattern_projection_enum() -> miette::Result<()> {
    cordial::init_tracing();
    let file = Path::new("transfer_error.rs");
    let ir = scan_verus_rust_source(
        DOCUMENTED_DATA_CARRYING_ENUM_SOURCE,
        file,
        "gallery::transfer_error",
    );

    let transfer_error = enum_named(&ir, "TransferError")?;
    assert!(transfer_error.synthesizes_pattern_projection_accessors());
    assert!(transfer_error.fully_documented());
    assert!(ir.is_documented_pattern_projection_enum(file, transfer_error.span.line()));
    Ok(())
}

#[test]
fn undocumented_data_carrying_variant_is_not_exempt() -> miette::Result<()> {
    cordial::init_tracing();
    let file = Path::new("transfer_error.rs");
    let ir = scan_verus_rust_source(
        UNDOCUMENTED_VARIANT_ENUM_SOURCE,
        file,
        "gallery::transfer_error",
    );

    let transfer_error = enum_named(&ir, "TransferError")?;
    assert!(transfer_error.synthesizes_pattern_projection_accessors());
    assert!(
        !transfer_error.fully_documented(),
        "NegativeAmount has no doc comment"
    );
    assert!(!ir.is_documented_pattern_projection_enum(file, transfer_error.span.line()));
    Ok(())
}

#[test]
fn unit_only_enum_never_synthesizes_accessors() -> miette::Result<()> {
    cordial::init_tracing();
    let file = Path::new("selector.rs");
    let ir = scan_verus_rust_source(UNIT_ONLY_ENUM_SOURCE, file, "gallery::selector");

    let selector = enum_named(&ir, "Selector")?;
    assert!(
        !selector.synthesizes_pattern_projection_accessors(),
        "no variant carries data"
    );
    assert!(!ir.is_documented_pattern_projection_enum(file, selector.span.line()));
    Ok(())
}

#[test]
fn wrong_line_or_file_is_never_a_match() -> miette::Result<()> {
    cordial::init_tracing();
    let file = Path::new("transfer_error.rs");
    let ir = scan_verus_rust_source(
        DOCUMENTED_DATA_CARRYING_ENUM_SOURCE,
        file,
        "gallery::transfer_error",
    );
    let transfer_error = enum_named(&ir, "TransferError")?;

    assert!(!ir.is_documented_pattern_projection_enum(file, transfer_error.span.line() + 1));
    assert!(
        !ir.is_documented_pattern_projection_enum(
            Path::new("other.rs"),
            transfer_error.span.line()
        )
    );
    Ok(())
}
