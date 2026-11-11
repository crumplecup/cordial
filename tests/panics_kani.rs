//! Panics reachable only from a Kani proof harness are not flagged.

use cordial::PanicKind;

#[path = "support/panic_sites.rs"]
mod panic_sites;

#[test]
fn unwrap_reachable_from_a_kani_proof_harness_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_14.rs"))?;
    assert!(
        findings.is_empty(),
        "unwrap() inside demonstrate_delivery is reachable from the #[kani::proof] harness \
         that calls it -- Kani checks reachable panics, not Result return values, so this \
         unwrap is the proof's own failure mechanism: {findings:?}"
    );
    Ok(())
}

#[test]
fn unwrap_reachable_from_a_harness_wrapped_in_another_macro_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    // amenable_derive::harness!(cfg_name, CONST_NAME, { item })'s own real
    // shape: syn::visit::Visit never descends into a macro invocation's
    // token stream on its own, so a #[kani::proof] fn written as a wrapper
    // macro's argument is invisible to root detection unless the scanner
    // goes looking for it -- this is the one real amenable_kani case that
    // motivated that lookup (every #[kani::proof] harness in that crate is
    // written this way, not as a bare item).
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_15.rs"))?;
    assert!(
        findings.is_empty(),
        "the harness! wrapper macro's braced argument must still be looked into to find the \
         #[kani::proof] fn inside it: {findings:?}"
    );
    Ok(())
}

#[test]
fn expect_reachable_only_via_an_assert_eq_argument_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    // Real amenable_kani shape: os_windows_model::kani_encode_wide_bmp_char's
    // only real caller passes it as assert_eq!'s own first argument, not as
    // a bare statement -- syn::visit::Visit never descends into ANY macro's
    // token stream on its own (assertion macros included), so this real
    // call edge was silently invisible to the reachability graph before the
    // fix that added exprs_inside_macro's fallback.
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_16.rs"))?;
    assert!(
        findings.is_empty(),
        "encode_bmp's real call site inside assert_eq!'s own arguments must still register a \
         reachability edge: {findings:?}"
    );
    Ok(())
}

#[test]
fn unreachable_nested_under_cfg_kani_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_17.rs"))?;
    assert!(
        findings.is_empty(),
        "code nested under #[cfg(kani)] exists only during Kani verification, so its \
         unreachable!() is part of the proof, not a library surface: {findings:?}"
    );
    Ok(())
}

#[test]
fn unwrap_not_reachable_from_any_kani_proof_is_still_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_18.rs"))?;
    assert_eq!(
        findings.len(),
        1,
        "ordinary_helper is never called by unrelated_proof -- its unwrap() is a real library \
         surface panic, not exempt just because the crate has some #[kani::proof] harness \
         somewhere: {findings:?}"
    );
    assert_eq!(findings[0].kind(), PanicKind::Unwrap);
    Ok(())
}

#[test]
fn panic_outside_cfg_kani_is_still_flagged_even_with_a_cfg_not_kani_twin() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_19.rs"))?;
    assert_eq!(
        findings.len(),
        1,
        "symbolic_any is called from a #[kani::proof] harness, but its panic!() lives in the \
         #[cfg(not(kani))] branch -- that code never runs during Kani verification, so it must \
         stay flagged rather than inherit its caller's kani-reachability: {findings:?}"
    );
    assert_eq!(findings[0].kind(), PanicKind::Panic);
    Ok(())
}
