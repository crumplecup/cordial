
use verus_builtin_macros::verus;

verus! {

pub fn leaf(x: i32) -> (result: i32)
    ensures
        result == x,
{
    let single_result = <char as std::str::FromStr>::from_str("a");
    let single_char = single_result.unwrap();
    let _ = single_char;
    x
}

pub fn has_a_real_caller(x: i32) -> (result: i32)
    ensures
        result == x,
{
    let single_result = <char as std::str::FromStr>::from_str("a");
    let single_char = single_result.unwrap();
    let _ = single_char;
    x
}

pub fn caller(x: i32) -> (result: i32)
{
    has_a_real_caller(x)
}

pub fn no_ensures_uncalled(x: i32) -> (result: i32)
{
    let single_result = <char as std::str::FromStr>::from_str("a");
    let single_char = single_result.unwrap();
    let _ = single_char;
    x
}

}
