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
//! **How.** No rustc integration here, so this can't do real trait
//! resolution. Scoped to what's actually needed: a single-file scan
//! (every real failing case found so far has the error type's own
//! definition in the same file as the function that returns it) for
//! `#[derive(..Display..)]` and `impl ..Display for X` -- plus a small,
//! deliberately narrow well-known-safe list for the handful of always-
//! `Display` types this codebase's own fixtures already rely on
//! (`String`, and a bare `Error` segment, e.g. `std::io::Error`). Any
//! `Err` type not positively confirmed is treated as **not** displayable
//! -- a missed `err()` recommendation is a minor omission, a proposed
//! `err()` that can't compile is a real bug.

use std::collections::HashMap;

use syn::{
    GenericArgument, Item, ItemEnum, ItemImpl, ItemStruct, ItemType, PathArguments, Type, TypePath,
};

use tracing::instrument;

/// Types (and `Result` aliases) known displayable within one file's own
/// scan, regardless of what other guarantees hold crate- or workspace-
/// wide.
const WELL_KNOWN_DISPLAY_TYPES: &[&str] = &["String", "Error"];

/// Per-file facts about which local types implement `Display` and which
/// local `type X<..> = Result<_, E>;` aliases resolve to a known `E`.
#[derive(Debug, Default, Clone)]
pub(super) struct DisplayTypeFacts {
    /// Last-path-segment names of locally defined types with a real
    /// `Display` impl (derived or hand-written) in this file.
    displayable: std::collections::HashSet<String>,
    /// Local `Result`-alias name -> its `Err` type's last-segment name,
    /// for `type X<..> = Result<T, E>;` items in this file.
    result_alias_err: HashMap<String, String>,
}

impl DisplayTypeFacts {
    /// `true` when `name` (a type's last path segment) is known
    /// displayable, locally or via the small well-known list.
    #[instrument(level = "trace", skip(self))]
    fn is_displayable(&self, name: &str) -> bool {
        WELL_KNOWN_DISPLAY_TYPES.contains(&name) || self.displayable.contains(name)
    }

    /// Whether `ty` (a function's return type) is `Result<_, E>` (direct
    /// or via a locally resolvable alias) with `E` known displayable.
    #[instrument(level = "trace", skip(self, ty))]
    pub(super) fn err_type_is_displayable(&self, ty: &Type) -> bool {
        err_type_name(ty, &self.result_alias_err).is_some_and(|name| self.is_displayable(&name))
    }
}

/// Scan one file's already-parsed items (recursing into inline modules)
/// for `Display` impls/derives and `Result`-alias definitions.
#[instrument(level = "debug", skip(items))]
pub(super) fn collect_display_type_facts(items: &[Item]) -> DisplayTypeFacts {
    let mut facts = DisplayTypeFacts::default();
    collect_from_items(items, &mut facts);
    facts
}

#[instrument(level = "debug", skip(items, facts))]
fn collect_from_items(items: &[Item], facts: &mut DisplayTypeFacts) {
    for item in items {
        match item {
            Item::Struct(ItemStruct { ident, attrs, .. }) => {
                if derives_display(attrs) {
                    facts.displayable.insert(ident.to_string());
                }
            }
            Item::Enum(ItemEnum { ident, attrs, .. }) => {
                if derives_display(attrs) {
                    facts.displayable.insert(ident.to_string());
                }
            }
            Item::Impl(item_impl) => record_display_impl(item_impl, facts),
            Item::Type(item_type) => record_result_alias(item_type, facts),
            Item::Mod(item_mod) => {
                if let Some((_, nested)) = &item_mod.content {
                    collect_from_items(nested, facts);
                }
            }
            _ => {}
        }
    }
}

#[instrument(level = "trace", skip(attrs), ret)]
fn derives_display(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            // `derive_more::Display`, not just a bare `Display` --
            // check the last path segment, matching how
            // `record_display_impl` reads a trait path below.
            if meta
                .path
                .segments
                .last()
                .is_some_and(|seg| seg.ident == "Display")
            {
                found = true;
            }
            Ok(())
        });
        found
    })
}

#[instrument(level = "debug", skip(item_impl, facts))]
fn record_display_impl(item_impl: &ItemImpl, facts: &mut DisplayTypeFacts) {
    let Some((_, trait_path, _)) = &item_impl.trait_ else {
        return;
    };
    if !trait_path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "Display")
    {
        return;
    }
    if let Some(name) = last_segment_name(&item_impl.self_ty) {
        facts.displayable.insert(name);
    }
}

#[instrument(level = "debug", skip(item_type, facts))]
fn record_result_alias(item_type: &ItemType, facts: &mut DisplayTypeFacts) {
    let Type::Path(TypePath { path, .. }) = item_type.ty.as_ref() else {
        return;
    };
    let Some(last) = path.segments.last() else {
        return;
    };
    if last.ident != "Result" {
        return;
    }
    if let Some(err_name) = direct_result_err_name(&last.arguments) {
        facts
            .result_alias_err
            .insert(item_type.ident.to_string(), err_name);
    }
}

/// The `Err` type's last-segment name for a return type written as
/// `Result<T, E>` directly, or via a locally defined `type X<..> =
/// Result<T, E>;` alias -- `None` when `E` isn't resolvable from this
/// file's own text (a foreign alias, a generic `Err` parameter, or a
/// bare alias name with no visible or resolvable second argument).
#[instrument(level = "debug", skip(ty, aliases))]
fn err_type_name(ty: &Type, aliases: &HashMap<String, String>) -> Option<String> {
    match ty {
        Type::Path(TypePath { path, .. }) => {
            let last = path.segments.last()?;
            let name = last.ident.to_string();
            if name == "Result" {
                direct_result_err_name(&last.arguments)
            } else {
                aliases.get(&name).cloned()
            }
        }
        Type::Reference(reference) => err_type_name(&reference.elem, aliases),
        Type::Paren(paren) => err_type_name(&paren.elem, aliases),
        Type::Group(group) => err_type_name(&group.elem, aliases),
        _ => None,
    }
}

/// `E`'s last-segment name from a literal `Result<T, E>`'s two angle-
/// bracketed type arguments -- `None` for a bare `Result` name with no
/// (or not exactly two) type arguments.
#[instrument(level = "debug", skip(args))]
fn direct_result_err_name(args: &PathArguments) -> Option<String> {
    let PathArguments::AngleBracketed(args) = args else {
        return None;
    };
    let type_args: Vec<&Type> = args
        .args
        .iter()
        .filter_map(|arg| match arg {
            GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })
        .collect();
    let [_, err_ty] = type_args[..] else {
        return None;
    };
    last_segment_name(err_ty)
}

#[instrument(level = "debug", skip(ty))]
fn last_segment_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(TypePath { path, .. }) => path.segments.last().map(|seg| seg.ident.to_string()),
        Type::Reference(reference) => last_segment_name(&reference.elem),
        Type::Paren(paren) => last_segment_name(&paren.elem),
        Type::Group(group) => last_segment_name(&group.elem),
        _ => None,
    }
}
