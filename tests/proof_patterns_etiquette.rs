use std::fs;

use cordial::{
    PROOF_PATTERNS_ETIQUETTE, ProofPatternKind, RunAll, Session, SessionBuilder,
    scan_crate_proof_patterns,
};
use miette::{IntoDiagnostic, WrapErr};

const FIXTURE_SOURCE: &str = r#"
use verus_builtin_macros::verus;

verus! {

axiom fn axiom_addition_commutes(a: int, b: int)
    ensures
        a + b == b + a,
{
}

uninterp spec fn opaque_hash(x: int) -> int;

proof fn trusts_a_local_claim(x: int)
    ensures
        x == x,
{
    assume(x == x);
}

proof fn discharges_unconditionally(x: int)
    ensures
        x == x,
{
    admit();
}

#[verifier::external_body]
fn opts_out_of_verification() -> (result: bool)
    ensures
        result,
{
    true
}

pub broadcast proof fn lemma_applies_everywhere(tracked cred: Cred)
    recommends
        cred.is_valid(),
    ensures
        true,
{
}

pub fn ordinary_fn(x: u32) -> (result: u32)
    ensures
        result == x,
{
    x
}

}
"#;

fn write_fixture(fixture: &std::path::Path) -> miette::Result<()> {
    fs::create_dir_all(fixture.join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.join("src/lib.rs"), FIXTURE_SOURCE)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    Ok(())
}

#[test]
fn scan_crate_finds_every_real_escape_hatch_and_broadcast() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_fixture(fixture.path())?;

    let records = scan_crate_proof_patterns(fixture.path())
        .into_diagnostic()
        .wrap_err("scan")?;

    let kinds: Vec<ProofPatternKind> = records.iter().map(|record| record.kind).collect();
    assert!(kinds.contains(&ProofPatternKind::Axiom), "{kinds:?}");
    assert!(kinds.contains(&ProofPatternKind::Uninterp), "{kinds:?}");
    assert!(kinds.contains(&ProofPatternKind::Assume), "{kinds:?}");
    assert!(kinds.contains(&ProofPatternKind::Admit), "{kinds:?}");
    assert!(kinds.contains(&ProofPatternKind::ExternalBody), "{kinds:?}");
    assert!(kinds.contains(&ProofPatternKind::Broadcast), "{kinds:?}");
    assert_eq!(kinds.len(), 6, "ordinary_fn must not be flagged: {records:?}");

    let broadcast = records
        .iter()
        .find(|record| record.kind == ProofPatternKind::Broadcast)
        .expect("broadcast record present");
    assert_eq!(broadcast.tracked_params, vec!["cred"]);
    assert_eq!(broadcast.recommends, vec!["cred . is_valid ()"]);
    Ok(())
}

#[test]
fn scan_skips_a_crate_with_no_verus_blocks() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn ok() {}\n")
        .into_diagnostic()
        .wrap_err("write fixture")?;

    let records = scan_crate_proof_patterns(fixture.path())
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(records.is_empty());
    Ok(())
}

#[test]
fn session_writes_checklist_from_real_fixture() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_fixture(fixture.path())?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&PROOF_PATTERNS_ETIQUETTE)
        .build();

    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    assert_eq!(outcome.findings().count(), 6);

    let findings_dir = store.path().join("findings");
    let csv = fs::read_to_string(findings_dir.join("proof-patterns.csv"))
        .into_diagnostic()
        .wrap_err("csv")?;
    assert!(csv.contains("PROOF-PATTERN-AXIOM"));
    assert!(csv.contains("PROOF-PATTERN-UNINTERP"));
    assert!(csv.contains("PROOF-PATTERN-ASSUME"));
    assert!(csv.contains("PROOF-PATTERN-ADMIT"));
    assert!(csv.contains("PROOF-PATTERN-EXTERNAL-BODY"));
    assert!(csv.contains("PROOF-PATTERN-BROADCAST"));
    assert!(csv.contains("cred"));

    let checklist = fs::read_to_string(findings_dir.join("proof-patterns.checklist.md"))
        .into_diagnostic()
        .wrap_err("checklist")?;
    assert!(checklist.contains("**Open items:** 6"));
    assert!(checklist.contains("tracked: cred"));
    assert!(checklist.contains("recommends: cred . is_valid ()"));

    let summary = fs::read_to_string(findings_dir.join("proof-patterns-summary.md"))
        .into_diagnostic()
        .wrap_err("summary")?;
    assert!(summary.contains("**5** trusted-not-proven"));
    assert!(summary.contains("**1** broadcast"));
    Ok(())
}
