//! Where a generic type's per-instantiation coverage comes from.

use std::collections::HashSet;

use crate::framework_std::type_identity::TypeKey;

/// What some coverage source knows about concrete instantiations of a generic
/// type, asked in terms of [`TypeKey`] identities.
///
/// The expansion into per-instantiation rows depends only on this trait. It
/// names no registry, wrapper type or verifier of its own, so any coverage
/// source can drive it: amenable's registry
/// ([`AmenableRegistryEvidence`](super::AmenableRegistryEvidence)) is one
/// implementation, and a source for another library or another notion of
/// coverage is another.
pub trait InstantiationEvidence {
    /// The concrete instantiations of the generic type `head`
    /// (`chrono::DateTime`) that the source has something to say about.
    fn instantiations_of(&self, head: &str) -> Vec<TypeKey>;

    /// The name of the evidence record about exactly `key`, if there is one.
    fn evidence_name_for(&self, key: &TypeKey) -> Option<String>;

    /// The verifiers that have a witness for exactly `key`.
    fn verifiers_for(&self, key: &TypeKey) -> HashSet<String>;

    /// Whether a test that exercises the proofs names exactly `key`.
    fn has_proof_test(&self, key: &TypeKey) -> bool;
}
