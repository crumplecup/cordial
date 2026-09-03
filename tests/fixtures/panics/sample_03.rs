
use verus_builtin_macros::verus;

verus! {

pub fn verify_int_error_kind_classifies_parse_failures(s: &str) -> (result: bool)
    requires
        s@.len() == 0,
    ensures
        result,
{
    match <i32 as std::str::FromStr>::from_str(s) {
        #[cfg(verus_keep_ghost)]
        Ok(_) => unreached(),
        #[cfg(not(verus_keep_ghost))]
        Ok(_) => unreachable!(),
        Err(_) => true,
    }
}

pub fn matches_on_result_with_no_ghost_sibling(x: i32) -> (result: bool)
{
    match x {
        0 => true,
        _ => unreachable!("no ghost sibling backs this one"),
    }
}

}
