//! Syntactic input and output paths for `From` implementations.

/// Type paths written in a `From<Input> for Output` implementation.
///
/// These paths have not undergone name resolution. An absent path means the
/// signature uses a shape such as a reference or associated type that this
/// inventory does not yet describe.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    derive_getters::Getters,
    derive_new::new,
)]
pub struct ErrorFromEvidence {
    trait_path: Vec<String>,
    input_type_path: Option<Vec<String>>,
    output_type_path: Option<Vec<String>>,
}
