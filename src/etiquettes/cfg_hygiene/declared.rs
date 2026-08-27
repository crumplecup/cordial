//! Resolves the set of cfg names one crate can legally gate on without
//! `unexpected_cfgs` firing, and (separately) each backend crate's own
//! configured verifier identity.
//!
//! Static source scanning only — this never invokes `rustc`/`cargo`, so it
//! can't see a `build.rs` that computes its `--check-cfg` list at runtime
//! (e.g. from an env var). That's an accepted limitation, same as every
//! other syn-based scanner in this crate: it reads what's on disk, not what
//! a real build would compute.

use std::collections::HashSet;
use std::path::Path;

use toml::Value;

use crate::config::CfgHygieneThresholds;

use tracing::instrument;
/// rustc's own fixed, versioned built-in `--check-cfg` vocabulary: always
/// "expected", with no project declaration needed. Verified against a real
/// `nightly` `rustc --print=check-cfg` on 2026-08-27:
///
/// ```sh
/// echo 'fn main() {}' > /tmp/dummy.rs
/// rustc +nightly -Zunstable-options --print=check-cfg \
///     --check-cfg 'cfg(placeholder)' /tmp/dummy.rs
/// ```
///
/// (`placeholder` is only there so the flag has something to combine with;
/// the other 32 names printed alongside it are rustc's own built-ins.)
/// Re-run that command to refresh this list if a future rustc adds or
/// removes one — `RUSTC_BUILTIN_NAMES.len()` documents the count so a
/// drift shows up as a test failure, not a silent gap.
const RUSTC_BUILTIN_NAMES: &[&str] = &[
    "clippy",
    "contract_checks",
    "debug_assertions",
    "doc",
    "doctest",
    "fmt_debug",
    "miri",
    "overflow_checks",
    "panic",
    "proc_macro",
    "relocation_model",
    "rustfmt",
    "sanitize",
    "sanitizer_cfi_generalize_pointers",
    "sanitizer_cfi_normalize_integers",
    "target_abi",
    "target_arch",
    "target_endian",
    "target_env",
    "target_family",
    "target_feature",
    "target_has_atomic",
    "target_has_atomic_load_store",
    "target_has_atomic_primitive_alignment",
    "target_has_threads",
    "target_object_format",
    "target_os",
    "target_pointer_width",
    "target_thread_local",
    "target_vendor",
    "ub_checks",
    "unix",
    "windows",
];

/// Cargo itself always injects these three on top of rustc's own list,
/// regardless of `Cargo.toml` — verified empirically
/// (`RUSTFLAGS="-D unexpected_cfgs" cargo check` on a throwaway crate,
/// 2026-08-27): `test` (every target, not just the unittest binary),
/// `feature` (the *name* is always expected; only an undeclared *value* is
/// flagged, which this etiquette doesn't check — see module docs), and
/// `docsrs` (the docs.rs convention, Cargo-injected since ~1.80).
const CARGO_INJECTED_NAMES: &[&str] = &["test", "feature", "docsrs"];

/// All cfg names `crate_root` can gate on without `unexpected_cfgs` firing:
/// [`RUSTC_BUILTIN_NAMES`] + [`CARGO_INJECTED_NAMES`] + this crate's own
/// `Cargo.toml [lints.rust.unexpected_cfgs.check-cfg]` +
/// `workspace_root`'s `[workspace.lints.rust...]` (only if this crate's
/// manifest sets `[lints] workspace = true`) + this crate's own
/// `build.rs`-emitted `cargo::rustc-check-cfg=cfg(...)` lines +
/// `thresholds`' `extra_known_names` escape hatch.
#[instrument(level = "debug", skip(thresholds))]
pub fn declared_names_for_crate(
    crate_root: &Path,
    workspace_root: &Path,
    thresholds: &CfgHygieneThresholds,
) -> HashSet<String> {
    let mut names: HashSet<String> = RUSTC_BUILTIN_NAMES
        .iter()
        .chain(CARGO_INJECTED_NAMES.iter())
        .map(|name| (*name).to_string())
        .collect();
    names.extend(thresholds.extra_known_names().iter().cloned());

    let manifest_path = crate_root.join("Cargo.toml");
    if let Ok(content) = std::fs::read_to_string(&manifest_path)
        && let Ok(table) = content.parse::<toml::Table>()
    {
        names.extend(check_cfg_names_from_package_table(&table));
        if uses_workspace_lints(&table) {
            let workspace_manifest = workspace_root.join("Cargo.toml");
            if let Ok(workspace_content) = std::fs::read_to_string(&workspace_manifest)
                && let Ok(workspace_table) = workspace_content.parse::<toml::Table>()
                && let Some(workspace_section) =
                    workspace_table.get("workspace").and_then(Value::as_table)
            {
                names.extend(check_cfg_names_from_package_table(workspace_section));
            }
        }
    }

    let build_script = crate_root.join("build.rs");
    if let Ok(content) = std::fs::read_to_string(&build_script) {
        names.extend(check_cfg_names_from_build_script(&content));
    }

    names
}

#[instrument(level = "debug", skip(table))]
fn uses_workspace_lints(table: &toml::Table) -> bool {
    table
        .get("lints")
        .and_then(Value::as_table)
        .and_then(|lints| lints.get("workspace"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Extracts every `cfg(NAME...)` entry's `NAME` from
/// `[lints.rust.unexpected_cfgs] check-cfg = [...]` in a package- or
/// `[workspace]`-rooted table.
#[instrument(level = "debug", skip(table))]
fn check_cfg_names_from_package_table(table: &toml::Table) -> Vec<String> {
    table
        .get("lints")
        .and_then(Value::as_table)
        .and_then(|lints| lints.get("rust"))
        .and_then(Value::as_table)
        .and_then(|rust| rust.get("unexpected_cfgs"))
        .and_then(Value::as_table)
        .and_then(|unexpected_cfgs| unexpected_cfgs.get("check-cfg"))
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .filter_map(extract_cfg_name)
                .collect()
        })
        .unwrap_or_default()
}

/// Extracts every declared name from a `build.rs`'s own source text, e.g.
/// `println!("cargo::rustc-check-cfg=cfg(kani)");` (current syntax) or the
/// legacy single-colon `cargo:rustc-check-cfg=cfg(kani)` form (still valid
/// on older `cargo`). A plain text scan, not a `syn` parse — the payload is
/// inside a string literal, not real Rust syntax.
#[instrument(level = "debug", skip(content))]
fn check_cfg_names_from_build_script(content: &str) -> Vec<String> {
    const MARKERS: &[&str] = &["cargo::rustc-check-cfg=", "cargo:rustc-check-cfg="];
    content
        .lines()
        .filter_map(|line| {
            MARKERS
                .iter()
                .find_map(|marker| line.split_once(marker).map(|(_, rest)| rest))
        })
        .filter_map(extract_cfg_name)
        .collect()
}

/// Extracts `NAME` from a `cfg(NAME)`/`cfg(NAME, values(...))` fragment,
/// however much trailing text follows (a rest-of-line remainder, a closing
/// `"` from a `println!` string literal, …).
#[instrument(level = "debug", skip(text))]
fn extract_cfg_name(text: &str) -> Option<String> {
    let after_cfg = text.split_once("cfg(")?.1;
    let name: String = after_cfg
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

/// This crate's own configured verifier identity from `cordial.toml`'s
/// `[cfg_hygiene] crate_verifier` table, if it's listed there at all —
/// `None` means CFG-VERIFIER-MISMATCH-001 doesn't apply to this crate
/// (deliberately opt-in per crate, not "every crate must own exactly one
/// verifier name"; see [`CfgHygieneThresholds::crate_verifier`] doc).
#[instrument(level = "debug", skip(thresholds))]
pub fn expected_verifier_for<'a>(
    thresholds: &'a CfgHygieneThresholds,
    crate_name: &str,
) -> Option<&'a str> {
    thresholds
        .crate_verifier()
        .get(crate_name)
        .map(String::as_str)
}

/// The distinct set of verifier cfg names across every crate registered in
/// `crate_verifier` — the vocabulary CFG-VERIFIER-MISMATCH-001 treats as
/// "belongs to some specific backend crate, not this one".
#[instrument(level = "debug", skip(thresholds))]
pub fn all_verifier_names(thresholds: &CfgHygieneThresholds) -> HashSet<&str> {
    thresholds
        .crate_verifier()
        .values()
        .map(String::as_str)
        .collect()
}
