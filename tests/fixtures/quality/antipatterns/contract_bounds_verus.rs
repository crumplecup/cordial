use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub fn verify_char_roundtrip(c: char) -> (result: char)
    ensures
        char_roundtrips(c),
{
    c
}

pub fn verify_something_raw(value: i32) -> (result: bool)
    ensures
        result == (value >= 0),
{
    value >= 0
}

} // verus!
