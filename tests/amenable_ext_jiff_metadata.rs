//! Step 1 regression coverage: `build_ext_etiquette("jiff")` must produce
//! metadata byte-identical to the pre-refactor hardcoded jiff etiquette
//! (see `docs/planning/amenable-ext-targets-config.md`, step 1's
//! acceptance bar). This is deliberately narrow — the probe/assessor/
//! reporter plumbing itself is unchanged, only the string construction
//! that derives ids/text from a target name is new, and that is exactly
//! what a mechanical generalization is most likely to get subtly wrong
//! (this test caught a real one: the explain page's rule summary lost
//! `ExtRowRule`'s capitalization of `target` until fixed alongside it).
#![cfg(feature = "amenable_ext")]

use cordial::{AMENABLE_EXT_JIFF_ETIQUETTE, Etiquette};

#[test]
fn jiff_etiquette_identity_matches_pre_refactor_constants() {
    cordial::init_tracing();
    let etiquette = &*AMENABLE_EXT_JIFF_ETIQUETTE;
    assert_eq!(etiquette.id(), "amenable-ext-jiff");
    assert_eq!(etiquette.name(), "Amenable ext (jiff) coverage");
    assert!(etiquette.is_coverage());
}

#[test]
fn jiff_etiquette_explain_text_matches_pre_refactor_constants() {
    cordial::init_tracing();
    let explain = AMENABLE_EXT_JIFF_ETIQUETTE.explain();
    assert_eq!(
        explain.summary(),
        "How much of jiff's registered carrier surface is in the amenable_ext registry?"
    );
    assert_eq!(
        explain.why(),
        "Same registry-coverage question amenable-std asks, for a third-party target crate via a shadow-dep rustdoc build instead of the shared sysroot cache."
    );
    assert_eq!(
        explain.logic(),
        "Workspace-scoped; no source loaders. Builds/reuses the target's shadow-dep rustdoc cache. Feature amenable_ext."
    );
    assert_eq!(
        explain.opt_out(),
        "`[amenable-ext-jiff] enabled = false` in cordial.toml."
    );
}

#[test]
fn jiff_etiquette_rule_text_matches_pre_refactor_constants_and_is_capitalized() {
    cordial::init_tracing();
    let explain = AMENABLE_EXT_JIFF_ETIQUETTE.explain();
    let rules = explain.rules();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].id(), "AMENABLE-EXT-JIFF-ROW");
    // Capitalized "Jiff", not the raw lowercase target -- this is exactly
    // the regression `ExtRowRule`'s `capitalize(target)` guards against,
    // now mirrored into the explain page instead of re-derived.
    assert_eq!(
        rules[0].summary(),
        "Jiff inventory row assessed for amenable_ext registry coverage"
    );
}
