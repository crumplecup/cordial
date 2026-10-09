//! Per-instantiation coverage rows for generic types.
//!
//! A generic type such as `chrono::DateTime<Tz>` is covered one concrete
//! instantiation at a time: `ExtStandard<T>` takes a concrete `T`, and each
//! instantiation has its own witnesses. This module turns a generic
//! inventory row into an aggregate parent plus one child row per
//! instantiation, each with its own status. Generic claims over a trait
//! what the type itself declares about its parameters is a note on the
//! parent, read from rustdoc. Design:
//! `docs/planning/amenable-ext-generic-instantiations.md`.

mod amenable_registry;
mod derive;
mod evidence;
mod expected;
mod note;
mod plan;

pub use amenable_registry::{
    AmenableRegistryEvidence, EvidenceKey, ProofKey, resolve_ext_evidence,
};
pub use evidence::InstantiationEvidence;
pub use expected::ExpectedInstantiations;
pub use plan::{InstantiationContext, expand_entries, expand_report};
