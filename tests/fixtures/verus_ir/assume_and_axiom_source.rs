
use verus_builtin_macros::verus;

verus! {

axiom fn axiom_addition_commutes(a: int, b: int)
    ensures
        a + b == b + a,
{
}

proof fn trusts_a_local_claim(x: int)
    ensures
        x == x,
{
    assume(x == x);
}

#[verifier::external_body]
fn opts_out_of_verification() -> (result: bool)
    ensures
        result,
{
    true
}

fn calls_admit_directly()
{
    admit();
}

}
