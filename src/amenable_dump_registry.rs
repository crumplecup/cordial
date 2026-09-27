//! The single source of truth for which cargo features `amenable
//! dump-registry` must be built with.
//!
//! Two independent consumers each need this list -- `framework_std`'s std/
//! ext coverage assessors, and the antipatterns `ANTIPATTERN-UNNAMED-
//! CONTRACT-BOUND-001` rule's own registry fetch -- and a hand-typed copy
//! per consumer already drifted out of sync once for real: the
//! antipatterns copy was missing `jiff`, so every `amenable_ext`/jiff
//! `ContractRecord`/`ProofRecord` registration was silently absent from
//! that rule's dump, and every correctly-named jiff contract call in
//! `amenable_kani`/`amenable_creusot`/`amenable_verus` misreported as
//! unnamed. One shared constant closes that drift risk for good.
//!
//! `std` and `amenable-ext-jiff` coverage share this one cached dump (see
//! each consumer's own `registry_dump_path`), so the list has to be a
//! superset of every active coverage plugin's needs, not just the one
//! that happens to run first -- `jiff` links `amenable_ext`'s
//! `ExtStandard<T>` registrations into the dump binary alongside
//! `creusot`/`verus`'s own `RustStdStandard<T>` witnesses. A future ext
//! target (e.g. chrono) adds its own activating feature name here too.
pub const AMENABLE_DUMP_REGISTRY_FEATURES: &str = "creusot,verus,jiff";
