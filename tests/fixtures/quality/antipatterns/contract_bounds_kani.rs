amenable_derive::harness! {
    kani, VERIFY_COMPLIANT_SRC, {
        #[kani::proof]
        fn verify_compliant() {
            let value: i32 = kani::any();
            kani::assume(fixture::NonNegative::requires(value));
            assert!(fixture::NonNegative::ensures(value), "message");
        }
    }
}

amenable_derive::harness! {
    kani, VERIFY_RAW_SRC, {
        #[kani::proof]
        fn verify_raw() {
            let value: i32 = kani::any();
            assert!(value < 100, "raw bound");
        }
    }
}
