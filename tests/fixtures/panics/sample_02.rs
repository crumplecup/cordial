
use verus_builtin_macros::verus;

verus! {

pub fn verify_try_from_int_error_occurs_exactly_when_out_of_range(value: i32) -> (result: bool)
    ensures
        result,
{
    match <u8 as std::convert::TryFrom<i32>>::try_from(value) {
        Ok(converted) => (0 <= value && value <= u8::MAX as i32) && converted as i32 == value,
        Err(_) => value < 0 || value > u8::MAX as i32,
    }
}

pub fn verify_int_error_kind_classifies_parse_failures(s: &str) -> (result: bool)
    requires
        s@.len() == 0,
    ensures
        result,
{
    match <i32 as std::str::FromStr>::from_str(s) {
        Ok(_) => unreachable!(),
        Err(_) => true,
    }
}

// A real caller: without one, this fn is an ensures-bearing verification
// leaf (see verus_reach), correctly exempt on its own -- this fixture's
// own point is that panics ARE still found inside verus! blocks, so it
// needs a non-leaf example.
pub fn calls_the_classifier(s: &str) -> bool {
    verify_int_error_kind_classifies_parse_failures(s)
}

}
