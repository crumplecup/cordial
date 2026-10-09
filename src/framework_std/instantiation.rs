//! Per-instantiation coverage rows for generic types.
//!
//! A generic type such as `chrono::DateTime<Tz>` is covered one concrete
//! instantiation at a time: `ExtStandard<T>` takes a concrete `T`, and each
//! instantiation has its own witnesses. This module turns a generic
//! inventory row into an aggregate parent plus one child row per
//! instantiation, each with its own status. Generic claims over a trait
//! bound (`ExtGeneric<..>`) stay on the parent as a note. Design:
//! `docs/planning/amenable-ext-generic-instantiations.md`.

mod expected;
mod facts;
mod note;
mod plan;

pub use expected::ExpectedInstantiations;
pub use facts::RegistryFacts;
pub use plan::{InstantiationContext, expand_report};
