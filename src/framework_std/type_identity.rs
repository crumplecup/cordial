//! Structured type identity for amenable-ext matching.
//!
//! Replaces string-suffix matching (`match_impl::type_has_trait_impl`) for
//! ext targets: evidence names and config entries are parsed to a
//! [`TypeKey`] and resolved through rustdoc, so every spelling of a type
//! (`chrono::Utc`, `chrono::offset::Utc`, a bare `Utc`) lands on one key.
//! See `docs/planning/amenable-ext-generic-instantiations.md`, phase A.

mod crate_index;
mod evidence;
mod key;
mod resolver;
mod text;

pub use crate_index::{CrateIndex, Lookup};
pub use evidence::{
    EvidenceKey, EvidenceKind, ProofKey, resolve_ext_evidence, resolve_ext_proofs,
    resolve_proof_subjects,
};
pub use key::TypeKey;
pub use resolver::{Implements, ResolveCaps, RustdocTypeResolver, TypeResolver, Unresolved};
pub use text::{TypeText, normalize_type_text, parse_type_text};
