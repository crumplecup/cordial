//! Adversarial syntax and proof contexts shared by both backend collectors.
trait Local {
    type Associated = Result<(), String>;
    fn ignored(_arg: String) -> Result<(), String>;
}
struct Carrier;
impl Local for Carrier {
    type Associated = Result<(), String>;
    fn ignored(_arg: String) -> Result<(), String> { Ok(()) }
}
unsafe extern "C" {
    fn foreign(_abi: i32) -> Result<(), String>;
}
#[requires(
    value@ >= 0
)]
#[ensures(predicate(result))]
fn creusot(value: i32) -> i32 { value }
#[kani::proof]
fn kani_shapes() {
    kani::assume(Contract::<i32, u8>::requires(3));
    assert!(
        <Carrier as Local>::ensures(3),
        "message, punctuation"
    );
    assert!(!Contract::<i32, u8>::ensures(3));
    assert_eq!(3, 4, "two args");
    assert!(#[trigger] predicate(3));
    fn not_a_proof() { assert!(false == true); }
}
verus! {
    mod inner {
        proof fn contract(value: int)
            requires value > 0,
            ensures #[trigger] predicate(value),
        {}
    }
}

amenable_derive::harness! {
    spec,
    {
        #[requires(outer_contract(value))]
        #[kani::proof]
        fn outer(value: i32) {
            assert!(outer_contract(value));
            mod nested {
              amenable_derive::harness! {
                nested,
                {
                    #[requires(inner_contract(value))]
                    #[kani::proof]
                    fn inner(value: i32) {
                        assert!(
                            inner_contract(value)
                        );
                        kani::assume(inner_contract(value));
                    }
                }
              }
            }
            amenable_derive::harness! { statement, {
                #[kani::proof]
                fn ignored_statement() { assert!(statement_contract(value)); }
            }}
        }
    }
}
