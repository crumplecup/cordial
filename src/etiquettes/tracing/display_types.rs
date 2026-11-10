//! Which `Result<_, E>` return types are safe to recommend `err(..)` for.
//!
//! **Why.** `#[instrument(err(level = ..))]` renders the `Err` payload via
//! `tracing_core::field::display`, which requires `E: std::fmt::Display`.
//! Recommending `err()` purely from "the function returns `Result`" (the
//! old behavior) proposes invalid code for any `Result<_, E>` where `E`
//! doesn't implement `Display` -- real precedent: `amenable_kani`'s own
//! accommodation-model error types (`KaniJoinPathsError`,
//! `KaniUtf8BufferError`, `StoplightError`, and others) derive `Debug`/
//! `PartialEq`/etc. but not `derive_more::Display`, so a real
//! `cordial quality --apply` run against them produced `E0277: doesn't
//! implement std::fmt::Display` across 9 distinct error types.
//!
//!
//! The per-file facts are collected from the shared tree in
//! `ir::tracing_source::display`; this module owns only the well-known
//! list both the facts and the policy consult.

/// Types (and `Result` aliases) known displayable within one file's own
/// scan, regardless of what other guarantees hold crate- or workspace-
/// wide.
pub(crate) const WELL_KNOWN_DISPLAY_TYPES: &[&str] = &["String", "Error"];
