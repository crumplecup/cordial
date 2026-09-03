
fn load_on_malformed_json_preserves_the_real_serde_error_in_the_chain(res: Result<(), AmenableError>) {
    let error = res.unwrap_err();
    let rendered = error.to_string();
    assert!(rendered.contains("line 1"));
}
