amenable_derive::harness! {
    creusot, VERIFY_CHAR_ROUNDTRIP_SRC, {
        #[requires(true)]
        #[ensures(char_roundtrips(c))]
        fn verify_char_roundtrip(c: char) -> char {
            c
        }
    }
}

amenable_derive::harness! {
    creusot, VERIFY_SOMETHING_RAW_SRC, {
        #[ensures(result >= 0)]
        fn verify_something_raw(value: i32) -> i32 {
            value
        }
    }
}

amenable_derive::harness! {
    creusot, VERIFY_PEARLITE_ONLY_SRC, {
        #[ensures(c@ <= 0xD7FF || (c@ >= 0xE000 && c@ <= 0x10FFFF))]
        fn verify_pearlite_only(c: char) -> char {
            c
        }
    }
}
