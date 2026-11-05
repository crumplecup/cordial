pub fn leaf() -> u8 { Some(2).unwrap() }
pub fn cycle_a() { cycle_b(); }
pub fn cycle_b() { cycle_a(); Some(2).unwrap(); }
pub fn runtime_branch() {
    #[cfg(not(kani))]
    { panic!("runtime branch"); }
    #[cfg(kani)]
    { Some(1).unwrap(); }
}
