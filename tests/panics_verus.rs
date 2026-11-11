//! Panic sites in Verus code: found inside blocks and embedded Rust, exempt when proven or in a verification leaf.

use cordial::PanicKind;

#[path = "support/panic_sites.rs"]
mod panic_sites;

#[test]
fn finds_panics_inside_a_verus_block() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_02.rs"))?;

    assert_eq!(
        findings.len(),
        1,
        "expected exactly the one real unreachable!() site: {findings:?}"
    );
    assert_eq!(findings[0].kind(), PanicKind::Unreachable);
    assert!(
        findings[0]
            .context()
            .ends_with("verify_int_error_kind_classifies_parse_failures"),
        "{:?}",
        findings[0].context()
    );
    Ok(())
}

#[cfg(feature = "verus_ir")]
#[test]
fn exempts_a_ghost_proven_unreachable_arm() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_03.rs"))?;

    assert_eq!(
        findings.len(),
        1,
        "the ghost-proven-unreachable site must be exempt, leaving only the unpaired one: {findings:?}"
    );
    assert_eq!(findings[0].kind(), PanicKind::Unreachable);
    assert!(
        findings[0]
            .context()
            .ends_with("matches_on_result_with_no_ghost_sibling"),
        "{:?}",
        findings[0].context()
    );
    Ok(())
}

#[cfg(feature = "verus_ir")]
#[test]
fn exempts_a_verification_leaf_but_not_a_helper_or_a_real_callee() -> miette::Result<()> {
    cordial::init_tracing();
    // Real amenable_verus shape: a `pub fn verify_*` with a real `ensures`
    // clause, called by nothing else in the crate, is itself the checked
    // claim -- its own .expect()/.unwrap() sites are that verification's
    // failure mechanism, not library API surface. Three functions here
    // isolate each condition: `leaf` (ensures + uncalled -> exempt),
    // `has_a_real_caller` (ensures, but called by `caller` -> NOT exempt),
    // and `no_ensures_uncalled` (uncalled, but no ensures -> NOT exempt).
    let findings = panic_sites::panic_sites(include_str!("fixtures/panics/sample_04.rs"))?;

    let contexts: Vec<&str> = findings.iter().map(|f| f.context().as_str()).collect();
    assert!(
        !contexts.iter().any(|c| c.ends_with("::leaf")),
        "leaf has ensures and no caller -- must be exempt: {contexts:?}"
    );
    assert!(
        contexts.iter().any(|c| c.ends_with("::has_a_real_caller")),
        "has_a_real_caller has ensures but IS called by caller -- must still be flagged: {contexts:?}"
    );
    assert!(
        contexts
            .iter()
            .any(|c| c.ends_with("::no_ensures_uncalled")),
        "no_ensures_uncalled has no ensures clause -- must still be flagged: {contexts:?}"
    );
    Ok(())
}

#[cfg(feature = "verus_ir")]
#[test]
fn finds_verus_abort_sites_inside_embedded_rust() -> miette::Result<()> {
    cordial::init_tracing();
    let source = format!(
        "fn host() {{ let _ = r####\"{}\"####; }}",
        include_str!("fixtures/panics/sample_02.rs")
    );
    let findings = panic_sites::panic_sites(&source)?;

    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].kind(), PanicKind::Unreachable);
    Ok(())
}
