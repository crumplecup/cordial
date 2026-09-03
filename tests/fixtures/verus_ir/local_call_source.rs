
use verus_builtin_macros::verus;

verus! {

pub fn helper(x: i32) -> (result: i32)
    ensures
        result == x,
{
    x
}

pub fn caller(x: i32) -> (result: i32)
    ensures
        result == x,
{
    let value = helper(x);
    let single_result = <char as std::str::FromStr>::from_str("a");
    assert(single_result.is_ok());
    value
}

}
