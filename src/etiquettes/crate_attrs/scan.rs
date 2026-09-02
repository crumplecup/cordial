//! Resolve the library root and inspect crate-level inner attributes.

use std::path::{Path, PathBuf};

use syn::punctuated::Punctuated;
use syn::{Attribute, File, Meta, MetaList, Token};

use crate::config::CrateAttrsThresholds;
use crate::error::CordialResult;

use super::types::{CrateAttrsRuleId, CrateAttrsSiteRecord};

#[cfg(feature = "verus_warnings")]
use crate::etiquettes::verus_warnings::crate_is_verus_target;

use tracing::instrument;

#[derive(Debug, Default, Clone, Copy)]
struct CrateLintPresence {
    forbid_unsafe_code: bool,
    missing_docs_at_least_warn: bool,
}

/// Scan the crate's library root for missing crate-level lint attributes.
#[instrument(level = "debug", skip(policy), err(level = "warn"))]
pub fn scan_crate_attrs(
    crate_root: &Path,
    crate_name: &str,
    policy: &CrateAttrsThresholds,
) -> CordialResult<Vec<CrateAttrsSiteRecord>> {
    let Some(lib) = library_root_rs(crate_root) else {
        return Ok(Vec::new());
    };

    let presence = if lib.is_file() {
        let source = std::fs::read_to_string(&lib)?;
        inspect_source(&source, &lib)?
    } else {
        CrateLintPresence::default()
    };

    let mut file = lib.clone();
    if let Ok(rel) = file.strip_prefix(crate_root) {
        file = rel.to_path_buf();
    }
    let line = 1;
    let mut records = Vec::new();
    if !policy.skip_unsafe(crate_name)
        && !presence.forbid_unsafe_code
        && !is_unsafe_forbid_exempt_verus_target(crate_root)
    {
        records.push(CrateAttrsSiteRecord {
            rule_id: CrateAttrsRuleId::ForbidUnsafe001,
            context: crate_name.to_string(),
            file: file.clone(),
            line,
            snippet: "add `#![forbid(unsafe_code)]`".to_string(),
        });
    }
    if !policy.skip_missing_docs(crate_name) && !presence.missing_docs_at_least_warn {
        records.push(CrateAttrsSiteRecord {
            rule_id: CrateAttrsRuleId::MissingDocs001,
            context: crate_name.to_string(),
            file,
            line,
            snippet: "add `#![warn(missing_docs)]`".to_string(),
        });
    }
    Ok(records)
}

/// A verus-target crate that states real `pub assume_specification
/// [..] (..) -> (..) ..;` claims about external functions is exempt from
/// `CRATE-FORBID-UNSAFE-001`: the real `verus` binary (invoked as a bare
/// compiler, never through `cargo`) expands `assume_specification` into
/// constructs its own `forbid(unsafe_code)` genuinely rejects
/// ("declaration of an unsafe function" / "usage of an unsafe block"),
/// even though the crate's own source never writes `unsafe` by hand --
/// confirmed empirically against a real `verus --crate-type=lib`
/// invocation, not assumed. Plain `cargo build`/clippy never see this;
/// only the real Verus toolchain does. Both signals required: a
/// verus-target crate that never actually states an external claim this
/// way has no reason to be exempt.
#[cfg(feature = "verus_warnings")]
#[instrument(level = "debug")]
fn is_unsafe_forbid_exempt_verus_target(crate_root: &Path) -> bool {
    crate_is_verus_target(crate_root) && crate_uses_assume_specification(crate_root)
}

#[cfg(not(feature = "verus_warnings"))]
#[instrument(level = "debug")]
fn is_unsafe_forbid_exempt_verus_target(_crate_root: &Path) -> bool {
    false
}

/// Whether any `.rs` file under `crate_root/src` literally mentions
/// `assume_specification` -- a plain substring search, not a real parse:
/// the construct only compiles under the real `verus` toolchain's own
/// extended grammar (`verus_syn`), which this scanner has no reason to
/// depend on just to recognize the one keyword that triggers the
/// `forbid(unsafe_code)` conflict this exemption exists for.
#[cfg(feature = "verus_warnings")]
#[instrument(level = "debug")]
fn crate_uses_assume_specification(crate_root: &Path) -> bool {
    let src_root = crate_root.join("src");
    if !src_root.is_dir() {
        return false;
    }
    walkdir::WalkDir::new(&src_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
        .any(|entry| {
            std::fs::read_to_string(entry.path())
                .is_ok_and(|source| source.contains("assume_specification"))
        })
}

/// Absolute path of this package's library root, if it has one.
#[instrument(level = "debug")]
pub fn library_root_rs(crate_root: &Path) -> Option<PathBuf> {
    let default = crate_root.join("src").join("lib.rs");
    let manifest = crate_root.join("Cargo.toml");
    if manifest.is_file()
        && let Ok(text) = std::fs::read_to_string(&manifest)
        && let Ok(parsed) = toml::from_str::<toml::Value>(&text)
    {
        if let Some(path) = parsed
            .get("lib")
            .and_then(|lib| lib.get("path"))
            .and_then(|path| path.as_str())
        {
            return Some(crate_root.join(path));
        }
        if parsed.get("lib").is_some() {
            return Some(default);
        }
    }
    default.is_file().then_some(default)
}

#[instrument(level = "debug", skip(source), err(level = "warn"))]
fn inspect_source(source: &str, file: &Path) -> CordialResult<CrateLintPresence> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    Ok(inspect_file(&syntax))
}

#[instrument(level = "debug", skip(file))]
fn inspect_file(file: &File) -> CrateLintPresence {
    let mut presence = CrateLintPresence::default();
    for attr in &file.attrs {
        apply_attr(attr, &mut presence);
    }
    presence
}

#[instrument(level = "debug", skip(attr, presence))]
fn apply_attr(attr: &Attribute, presence: &mut CrateLintPresence) {
    match &attr.meta {
        Meta::List(list) if is_lint_level(&list.path, "forbid") => {
            apply_level("forbid", list, presence);
        }
        Meta::List(list) if is_lint_level(&list.path, "deny") => {
            apply_level("deny", list, presence);
        }
        Meta::List(list) if is_lint_level(&list.path, "warn") => {
            apply_level("warn", list, presence);
        }
        Meta::List(list) if list.path.is_ident("cfg_attr") => {
            apply_cfg_attr(list, presence);
        }
        _ => {}
    }
}

#[instrument(level = "debug", skip(path))]
fn is_lint_level(path: &syn::Path, level: &str) -> bool {
    path.is_ident(level)
}

#[instrument(level = "debug", skip(list, presence))]
fn apply_level(level: &str, list: &MetaList, presence: &mut CrateLintPresence) {
    for name in nested_lint_names(list) {
        match (level, name.as_str()) {
            ("forbid", "unsafe_code") => presence.forbid_unsafe_code = true,
            ("forbid" | "deny" | "warn", "missing_docs") => {
                presence.missing_docs_at_least_warn = true;
            }
            _ => {}
        }
    }
}

#[instrument(level = "debug", skip(list, presence))]
fn apply_cfg_attr(list: &MetaList, presence: &mut CrateLintPresence) {
    let Ok(nested) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated) else {
        return;
    };
    for meta in nested.into_iter().skip(1) {
        let Meta::List(inner) = meta else {
            continue;
        };
        if inner.path.is_ident("forbid") {
            apply_level("forbid", &inner, presence);
        } else if inner.path.is_ident("deny") {
            apply_level("deny", &inner, presence);
        } else if inner.path.is_ident("warn") {
            apply_level("warn", &inner, presence);
        }
    }
}

#[instrument(level = "debug", skip(list))]
fn nested_lint_names(list: &MetaList) -> Vec<String> {
    let Ok(nested) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated) else {
        return Vec::new();
    };
    nested
        .into_iter()
        .filter_map(|meta| match meta {
            Meta::Path(path) => path.segments.last().map(|seg| seg.ident.to_string()),
            Meta::List(inner) => inner.path.segments.last().map(|seg| seg.ident.to_string()),
            Meta::NameValue(nv) => nv.path.segments.last().map(|seg| seg.ident.to_string()),
        })
        .collect()
}
