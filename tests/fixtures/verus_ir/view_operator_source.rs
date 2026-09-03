
use verus_builtin_macros::verus;

verus! {

pub open spec fn non_nul_byte_value_is_nonzero(byte: u8) -> bool {
    byte != 0
}

pub fn verify_cstr_excludes_the_terminating_nul_from_to_bytes(byte: u8) -> (result: bool)
    requires
        non_nul_byte_value_is_nonzero(byte),
    ensures
        result,
{
    let with_nul: &[u8] = &[byte, 0];
    let cstr_result = CStr::from_bytes_with_nul(with_nul);
    assert(cstr_result is Ok);
    let cstr = cstr_result.unwrap();

    let bytes = cstr.to_bytes();
    assert(bytes@.len() == 1);
    assert(bytes@[0] == byte);
    true
}

}
