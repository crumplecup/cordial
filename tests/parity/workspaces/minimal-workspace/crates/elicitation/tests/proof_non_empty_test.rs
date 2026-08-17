//! Minimal proof harness fixture for pipeline extract tests.

#[test]
fn proof_fixture() {
    assert_proofs_non_empty::<url::Widget>();
}

fn assert_proofs_non_empty<T>() {}
