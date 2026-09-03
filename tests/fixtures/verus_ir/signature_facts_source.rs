
use verus_builtin_macros::verus;

verus! {

pub broadcast proof fn lemma_applies_everywhere(tracked cred: Cred)
    recommends
        cred.is_valid(),
    ensures
        true,
{
}

pub fn matches_on_result(x: i32) -> (result: bool)
{
    match x {
        0 => panic!("zero"),
        1 => unreachable!(),
        _ => x.checked_div(2).expect("nonzero"),
    };
    x.checked_div(2).unwrap_err().is_some()
}

}
