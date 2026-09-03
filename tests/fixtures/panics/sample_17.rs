
#[cfg(kani)]
mod proofs {
    fn from_index(index: u8) -> bool {
        match index {
            0 => false,
            1 => true,
            _ => unreachable!("bounded by kani::assume"),
        }
    }

    #[kani::proof]
    fn verify_from_index() {
        let index: u8 = kani::any();
        kani::assume(index <= 1);
        from_index(index);
    }
}
