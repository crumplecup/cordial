
use verus_builtin_macros::verus;

verus! {

/// Sanitized mirror of a real error type.
pub enum TransferError {
    NegativeAmount(i64),
    /// The paying and receiving accounts were the same.
    SameAccount,
}

}
