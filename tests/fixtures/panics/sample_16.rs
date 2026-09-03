
pub fn encode_bmp(c: char) -> u16 {
    (c as u32).try_into().expect("c is a BMP scalar value by this fn's own precondition")
}

#[kani::proof]
fn verify_encode_bmp() {
    let c: char = kani::any();
    kani::assume((c as u32) < 0x10000);
    assert_eq!(encode_bmp(c), c as u32 as u16, "message");
}
