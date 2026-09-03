
use verus_builtin_macros::verus;

verus! {

pub fn matches_int_error_kind_carriers_own_shape(s: &str) -> (result: bool)
{
    match <i32 as std::str::FromStr>::from_str(s) {
        #[cfg(verus_keep_ghost)]
        Ok(_) => unreached(),
        #[cfg(not(verus_keep_ghost))]
        Ok(_) => unreachable!(),
        Err(_) => true,
    }
}

pub fn ordinary_unreachable_with_no_ghost_sibling(x: u32) -> (result: u32)
{
    match x {
        0 => 0,
        _ => unreachable!("no ghost sibling backs this one"),
    }
}

}
