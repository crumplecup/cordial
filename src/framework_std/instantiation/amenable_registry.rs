//! Amenable's registry as an [`InstantiationEvidence`] source.
//!
//! The one place that knows amenable's naming: evidence is registered as
//! `ExtStandard<T>` (qualified or bare), proof records and proof-chain tests
//! name the same wrapper. Everything here resolves those names to
//! [`TypeKey`]s once, so a lookup compares identities, not strings.

use std::collections::HashSet;

use tracing::instrument;

use crate::framework_std::registry::{EvidenceLinkDump, RegistryDump};
use crate::framework_std::type_identity::{
    TypeKey, TypeResolver, Unresolved, normalize_type_text, parse_type_text,
};

use super::InstantiationEvidence;

const EXT_STANDARD: &[&str] = &["amenable_ext::ExtStandard", "ExtStandard"];

/// One registry evidence link, resolved.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct EvidenceKey {
    /// The link's name as registered.
    name: String,
    /// The type it is about, or why that type could not be identified.
    key: Result<TypeKey, Unresolved>,
}

/// One registry proof record, resolved to the type it witnesses.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ProofKey {
    /// The type the proof is about, or why it could not be identified.
    key: Result<TypeKey, Unresolved>,
    /// Which verifier produced it (`kani`, `creusot`, `verus`).
    verifier: String,
}

/// Everything amenable's registry says about ext types, resolved once.
#[derive(Debug, Clone)]
pub struct AmenableRegistryEvidence {
    evidence: Vec<EvidenceKey>,
    proofs: Vec<ProofKey>,
    subjects: Vec<TypeKey>,
}

impl AmenableRegistryEvidence {
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
}

impl InstantiationEvidence for AmenableRegistryEvidence {
    #[instrument(level = "trace", skip(self))]
    fn instantiations_of(&self, head: &str) -> Vec<TypeKey> {
        self.evidence
            .iter()
            .filter_map(|entry| entry.key().as_ref().ok())
            .filter(|key| key.head() == head && !key.args().is_empty())
            .cloned()
            .collect()
    }

    #[instrument(level = "trace", skip(self, key))]
    fn evidence_name_for(&self, key: &TypeKey) -> Option<String> {
        self.evidence
            .iter()
            .find(|entry| entry.key().as_ref().is_ok_and(|found| found == key))
            .map(|entry| entry.name().clone())
    }

    #[instrument(level = "trace", skip(self, key))]
    fn verifiers_for(&self, key: &TypeKey) -> HashSet<String> {
        self.proofs
            .iter()
            .filter(|proof| proof.key().as_ref().is_ok_and(|found| found == key))
            .map(|proof| proof.verifier().clone())
            .collect()
    }

    #[instrument(level = "trace", skip(self, key))]
    fn has_proof_test(&self, key: &TypeKey) -> bool {
        self.subjects.iter().any(|subject| subject == key)
    }
}

/// Resolve every `ExtStandard<…>` link in `registry`.
///
/// Links with any other wrapper (`RustStdStandard<…>`, bare evidence) are not
/// ext claims and are skipped. Nothing is dropped silently: a link whose type
/// cannot be identified is returned with its [`Unresolved`] reason.
#[instrument(level = "debug", skip(registry, resolver))]
pub fn resolve_ext_evidence(
    registry: &RegistryDump,
    resolver: &dyn TypeResolver,
) -> Vec<EvidenceKey> {
    registry
        .evidence_links()
        .iter()
        .filter_map(|link| resolve_link(link, resolver))
        .collect()
}

#[instrument(level = "debug", skip(link, resolver))]
fn resolve_link(link: &EvidenceLinkDump, resolver: &dyn TypeResolver) -> Option<EvidenceKey> {
    // Recognize an ext claim from the wrapper alone, before parsing: a link we
    // recognize but cannot parse must still be reported.
    let normalized = normalize_type_text(link.name());
    let wrapper = normalized.split('<').next().unwrap_or_default();
    if !EXT_STANDARD.contains(&wrapper) {
        return None;
    }
    let key = match parse_type_text(&normalized) {
        Some(parsed) => match parsed.args().as_slice() {
            [inner] => resolver.resolve(&inner.to_string()),
            _ => Err(Unresolved::Malformed(link.name().clone())),
        },
        None => Err(Unresolved::Malformed(link.name().clone())),
    };
    Some(EvidenceKey {
        name: link.name().clone(),
        key,
    })
}

/// Resolve every proof record whose evidence is an `ExtStandard<..>` claim.
#[instrument(level = "debug", skip(registry, resolver))]
pub fn resolve_ext_proofs(registry: &RegistryDump, resolver: &dyn TypeResolver) -> Vec<ProofKey> {
    registry
        .proof_records()
        .iter()
        .filter_map(|record| {
            let inner = ext_standard_inner(record.evidence())?;
            Some(ProofKey {
                key: match inner {
                    Some(text) => resolver.resolve(&text),
                    None => Err(Unresolved::Malformed(record.evidence().clone())),
                },
                verifier: record.verifier().clone(),
            })
        })
        .collect()
}

/// Resolve proof-chain test subjects: the type each `proof_chain_test.rs`
/// source names, whether wrapped in `ExtStandard<..>` or written bare.
/// Subjects that cannot be identified are left out; they witness nothing.
#[instrument(level = "debug", skip(subjects, resolver))]
pub fn resolve_proof_subjects(
    subjects: &HashSet<String>,
    resolver: &dyn TypeResolver,
) -> Vec<TypeKey> {
    subjects
        .iter()
        .filter_map(|subject| {
            let text = match ext_standard_inner(subject) {
                Some(Some(inner)) => inner,
                Some(None) => return None,
                None => normalize_type_text(subject),
            };
            resolver.resolve(&text).ok()
        })
        .collect()
}

/// The type text inside an `ExtStandard<..>` name: `None` when the name is
/// not one, `Some(None)` when it is one but malformed.
#[instrument(level = "trace")]
fn ext_standard_inner(name: &str) -> Option<Option<String>> {
    let normalized = normalize_type_text(name);
    let wrapper = normalized.split('<').next().unwrap_or_default();
    if !EXT_STANDARD.contains(&wrapper) {
        return None;
    }
    let parsed = parse_type_text(&normalized);
    Some(match parsed.as_ref().map(|text| text.args().as_slice()) {
        Some([inner]) => Some(inner.to_string()),
        _ => None,
    })
}
