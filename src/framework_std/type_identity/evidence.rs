//! Resolve registry evidence names to structured type identities.

use tracing::instrument;

use crate::framework_std::registry::{EvidenceLinkDump, RegistryDump};

use super::{TypeKey, TypeResolver, Unresolved, normalize_type_text, parse_type_text};

const EXT_STANDARD: &[&str] = &["amenable_ext::ExtStandard", "ExtStandard"];
const EXT_GENERIC: &[&str] = &["amenable_ext::ExtGeneric", "ExtGeneric"];

/// Whether an evidence claim covers one concrete type or a generic family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    /// `ExtStandard<T>`: a claim about exactly the type `T`.
    Concrete,
    /// `ExtGeneric<T<..>>`: a claim over every type satisfying its bounds.
    /// Only the head is identified; the arguments are type parameters.
    Generic,
}

/// One registry evidence link, resolved.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct EvidenceKey {
    /// The link's name as registered.
    name: String,
    /// The wrapper the link uses.
    #[getter(copy)]
    kind: EvidenceKind,
    /// The type it is about, or why that type could not be identified. A
    /// generic claim resolves its head only.
    key: Result<TypeKey, Unresolved>,
    /// Trait bounds a generic claim is generic over; empty for a concrete one.
    bounds: Vec<String>,
}

/// Resolve every `ExtStandard<…>` and `ExtGeneric<…>` link in `registry`.
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
    let normalized = normalize_type_text(link.name());
    // Decide whether this is an ext claim from the wrapper alone, before
    // parsing: a link we recognize but cannot parse must still be reported.
    let wrapper = normalized.split('<').next().unwrap_or_default();
    let kind = if EXT_STANDARD.contains(&wrapper) {
        EvidenceKind::Concrete
    } else if EXT_GENERIC.contains(&wrapper) {
        EvidenceKind::Generic
    } else {
        return None;
    };
    let key = match parse_type_text(&normalized) {
        Some(parsed) => match parsed.args().as_slice() {
            [inner] => match kind {
                EvidenceKind::Concrete => resolver.resolve(&inner.to_string()),
                EvidenceKind::Generic => resolver.resolve_head(&inner.to_string()),
            },
            _ => Err(Unresolved::Malformed(link.name().clone())),
        },
        None => Err(Unresolved::Malformed(link.name().clone())),
    };
    Some(EvidenceKey {
        name: link.name().clone(),
        kind,
        key,
        bounds: link.bounds().clone(),
    })
}
