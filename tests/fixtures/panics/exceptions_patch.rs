pub fn boom() {
    panic!("kaboom");
}

pub fn fragile() -> u32 {
    Some(1).expect("missing")
}
