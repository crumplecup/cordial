
fn clap_rejects_a_single_proof_combined_with_a_retry_selector(res: Result<Cli, ClapError>) {
    let error = build_cli([
        "amenable",
        "verify",
        "kani",
    ])
    .expect_err("conflicting selectors must be rejected");

    assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
}
