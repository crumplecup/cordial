
pub fn ordinary_helper(value: Option<i32>) -> i32 {
    value.unwrap()
}

#[kani::proof]
fn unrelated_proof() {
    let _ = 1 + 1;
}
