//! Resolve registry evidence names to structured type identities.

use std::collections::HashSet;

use tracing::instrument;

use crate::framework_std::registry::{EvidenceLinkDump, RegistryDump};

use super::{TypeKey, TypeResolver, Unresolved, normalize_type_text, parse_type_text};

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
