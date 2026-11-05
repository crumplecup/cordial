mod leaf;
mod middle;

#[kani::proof]
fn root() {
    assert_eq!(middle::middle(), 2);
    leaf::runtime_branch();
}

amenable_derive::harness!(cfg_cycle, SOURCE, {
    #[kani::proof]
    fn wrapped() { leaf::cycle_a(); }
});

pub fn manual() { Some(1).expect("manual repair"); }

pub fn fixture_host() {
    let _ = r#"fn embedded() { panic!("embedded"); }"#;
}

#[cfg(test)]
mod tests {
    fn error_assertion() {
        let error = Result::<(), u8>::Err(1).unwrap_err();
        let rendered = error.to_string();
        assert_eq!(rendered, "1");
    }
    fn setup() { Some(1).expect("test setup"); }
}
