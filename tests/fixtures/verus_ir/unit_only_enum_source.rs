
use verus_builtin_macros::verus;

verus! {

/// Selects which composed claim a call proves.
pub enum Selector {
    /// The first arm.
    Balanced,
    /// The second arm.
    Closed,
}

}
