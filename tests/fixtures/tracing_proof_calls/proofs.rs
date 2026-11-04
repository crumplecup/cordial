use leaf::*;
#[cfg(kani)]
mod proofs {
    fn harness() {
        bridge();
        mixed();
        assert!(Widget::<u8>::ensures());
        Contract::check();
        fn nested() { leaf(); }
        let closure = || leaf();
        opaque!(opaque_only());
    }
}
fake_harness_macro::harness! {
    kani, SUITE, {
        #[kani::proof]
        fn macro_proof() {
            assert!(assertion_target(), "{}", message_only());
            debug_assert_eq!(1, 2, "{}", message_only());
            assert!(assert_eq!(assertion_target(), 1));
        }
        mod nested {
            nested_harness! {
                args, {
                    #[kani::proof]
                    fn nested_macro_proof() { assertion_target(); }
                }
            }
        }
    }
}
#[cfg(any(kani))]
fn composite_is_not_bare_gate() { opaque_only(); }
#[cfg(other)]
mod inactive {
    fn dormant() { message_only(); }
}
