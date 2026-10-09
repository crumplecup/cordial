//! The registry, resolved once into structured keys.

use std::collections::HashSet;

use tracing::instrument;

use crate::framework_std::registry::RegistryDump;
use crate::framework_std::type_identity::{
    EvidenceKey, ProofKey, TypeKey, TypeResolver, resolve_ext_evidence, resolve_ext_proofs,
    resolve_proof_subjects,
};

/// Everything the registry says about ext types, resolved to [`TypeKey`]s so
/// a lookup compares identities, not strings. Resolving the registry is the
/// expensive step, so it is done once and queried per instantiation.
#[derive(Debug, Clone)]
pub struct RegistryFacts {
    evidence: Vec<EvidenceKey>,
    proofs: Vec<ProofKey>,
    subjects: Vec<TypeKey>,
}

impl RegistryFacts {
    /// Resolve `registry` and the proof-chain test `subjects` with `resolver`.
    #[instrument(level = "debug", skip(registry, subjects, resolver))]
    pub fn resolve(
        registry: &RegistryDump,
        subjects: &HashSet<String>,
        resolver: &dyn TypeResolver,
    ) -> Self {
        Self {
            evidence: resolve_ext_evidence(registry, resolver),
            proofs: resolve_ext_proofs(registry, resolver),
            subjects: resolve_proof_subjects(subjects, resolver),
        }
    }

    /// Concrete claims about an instantiation of the generic type `head`.
    #[instrument(level = "trace", skip(self))]
    pub fn instantiations_of(&self, head: &str) -> Vec<&TypeKey> {
        self.evidence
            .iter()
            .filter_map(|entry| entry.key().as_ref().ok())
            .filter(|key| key.head() == head && !key.args().is_empty())
            .collect()
    }

    /// The name of the concrete evidence link about exactly `key`.
    #[instrument(level = "trace", skip(self, key))]
    pub fn evidence_name_for(&self, key: &TypeKey) -> Option<String> {
        self.evidence
            .iter()
            .find(|entry| entry.key().as_ref().is_ok_and(|found| found == key))
            .map(|entry| entry.name().clone())
    }

    /// Verifiers that have a witness for exactly `key`.
    #[instrument(level = "trace", skip(self, key))]
    pub fn verifiers_for(&self, key: &TypeKey) -> HashSet<String> {
        self.proofs
            .iter()
            .filter(|proof| proof.key().as_ref().is_ok_and(|found| found == key))
            .map(|proof| proof.verifier().clone())
            .collect()
    }

    /// Whether a proof-chain test names exactly `key`.
    #[instrument(level = "trace", skip(self, key))]
    pub fn has_proof_test(&self, key: &TypeKey) -> bool {
        self.subjects.iter().any(|subject| subject == key)
    }
}
