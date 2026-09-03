fn holds_a_sample() {
    let _ = r#"
pub fn boom() {
    panic!("from-string");
}
"#;
}
