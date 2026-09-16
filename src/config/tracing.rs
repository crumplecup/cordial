use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::instrument;

use super::default_true;

/// Tracing etiquette knobs.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct TracingThresholds {
    /// Extra parameter names unioned with the built-in skip list.
    #[serde(default)]
    extra_skip: Vec<String>,
    /// Crate name -> cfg name. `--apply` wraps `#[instrument(..)]` as
    /// `#[cfg_attr(not(#cfg), instrument(..))]` for every function in
    /// this crate (and any crate that transitively depends on it, since
    /// e.g. `cargo kani`'s `--cfg kani` applies to the whole dependency
    /// graph it compiles, not just the top-level target crate) --
    /// real precedent: a bare `#[instrument]` on any function reachable
    /// from a `#[kani::proof]` harness causes real CBMC symbolic-
    /// closure-capture timeouts (confirmed via a real gallery
    /// experiment in a sibling project's own prior art), not just risks
    /// one.
    #[serde(default)]
    apply_gate_crates: std::collections::HashMap<String, String>,
    /// Crate names `--apply` never writes `#[instrument]` into at all,
    /// leaving the checklist item open -- for a crate whose real
    /// toolchain either can't resolve the `tracing` crate at all (a
    /// bare-compiler invocation that never reads `Cargo.toml`, real
    /// precedent: `verus --crate-type=lib`), or hard-fails compilation
    /// on `#[instrument]`'s own expansion (real precedent: Creusot's
    /// translator can't handle the static `DefaultCallsite` reference
    /// `tracing::span!` embeds, confirmed via a real `cargo creusot`
    /// run, not assumed from a milder "generated companions only" read
    /// of the failure). Unlike `apply_gate_crates`, this does **not**
    /// propagate through the ordinary dependency graph -- a translator
    /// that only sweeps a crate's own local items has no reason to
    /// touch an ordinary dependency's source at all (real precedent:
    /// `creusot-rustc`) -- but it does propagate through a `#[path]`
    /// splice, since that copies the physical file's real content into
    /// the splicing crate's own compilation unit.
    #[serde(default)]
    apply_skip_crates: Vec<String>,
    /// Subscriber-init policy knobs. Each defaults **on**.
    #[serde(default)]
    #[new(default)]
    subscriber: TracingSubscriberPolicy,
    /// Binary error-boundary policy knobs. Defaults **on**.
    #[serde(default)]
    #[new(default)]
    boundary: TracingBoundaryPolicy,
    /// Leftover-stdio filter. Each macro defaults **on**.
    #[serde(default)]
    #[new(default)]
    stdio: TracingStdioPolicy,
    /// Run this etiquette (`true`) or skip it (`false`).
    #[serde(default = "default_true")]
    #[new(value = "true")]
    #[getter(copy)]
    enabled: bool,
}

/// Whether each tracing-subscriber init rule is armed.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct TracingSubscriberPolicy {
    /// `fn main` in a binary must call the crate's init helper.
    #[serde(default = "default_true")]
    #[getter(copy)]
    init_in_main: bool,
    /// Each `#[test]` under `tests/` must call the same helper.
    #[serde(default = "default_true")]
    #[getter(copy)]
    init_in_tests: bool,
    /// The function that builds/installs the subscriber lives in the library.
    #[serde(default = "default_true")]
    #[getter(copy)]
    helper_in_lib: bool,
    /// That helper reads `RUST_LOG` and has a fallback (not `from_default_env()` alone).
    #[serde(default = "default_true")]
    #[getter(copy)]
    rust_log_fallback: bool,
    /// That helper uses `try_init()` or wraps `init()` in `Once` / `OnceLock`.
    #[serde(default = "default_true")]
    #[getter(copy)]
    idempotent: bool,
    /// Fully-qualified paths (e.g. `amenable_core::init_tracing`) of a
    /// shared helper defined in one crate and called from a sibling
    /// crate's `main`/`#[test]` -- a real, common shape in a multi-crate
    /// workspace that a single-crate scan can never verify on its own
    /// (the helper's *defining* crate is scanned separately, and its own
    /// body is checked there via `helper_in_lib`/`rust_log_fallback`/
    /// `idempotent`). A call matching one of these is trusted as a
    /// complete, compliant install; empty by default (inert until a
    /// project actually has a cross-crate helper).
    #[serde(default)]
    known_helper_paths: Vec<String>,
}

impl Default for TracingSubscriberPolicy {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            init_in_main: true,
            init_in_tests: true,
            helper_in_lib: true,
            rust_log_fallback: true,
            idempotent: true,
            known_helper_paths: Vec::new(),
        }
    }
}

/// Whether the binary error-boundary rule is armed.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct TracingBoundaryPolicy {
    /// A fallible `fn main` in a binary must convert its error to a
    /// tracing warn/error emission (via `#[instrument(err(...))]` or an
    /// explicit `tracing::warn!`/`error!` on the error path) before the
    /// process boundary, instead of letting it bubble up and crash.
    #[serde(default = "default_true")]
    #[getter(copy)]
    main_reports_errors: bool,
    /// Fully-qualified paths (e.g. `amenable_core::run_and_report`) of a
    /// shared dispatch helper defined in one crate and called from a
    /// sibling crate's `main` -- trusted as already reporting its own
    /// errors, the same way `[tracing.subscriber] known_helper_paths`
    /// trusts a cross-crate init helper. Empty by default.
    #[serde(default)]
    known_helper_paths: Vec<String>,
}

impl Default for TracingBoundaryPolicy {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            main_reports_errors: true,
            known_helper_paths: Vec::new(),
        }
    }
}

/// Whether each leftover-stdio macro is armed, plus folder / cargo skips.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub struct TracingStdioPolicy {
    /// Flag leftover `println!` (including `std::println!`).
    #[serde(default = "default_true")]
    #[getter(copy)]
    println: bool,
    /// Flag leftover `eprintln!`.
    #[serde(default = "default_true")]
    #[getter(copy)]
    eprintln: bool,
    /// Flag leftover `print!`.
    #[serde(default = "default_true")]
    #[getter(copy)]
    print: bool,
    /// Flag leftover `eprint!`.
    #[serde(default = "default_true")]
    #[getter(copy)]
    eprint: bool,
    /// Flag leftover `dbg!`.
    #[serde(default = "default_true")]
    #[getter(copy)]
    dbg: bool,
    /// Skip first-string `cargo:` / `cargo::` build-script protocol.
    #[serde(default = "default_true")]
    #[getter(copy)]
    skip_cargo_protocol: bool,
    /// Crate-relative folder prefixes to skip (`tests/fixtures`, `src/generated`).
    /// Replacing this list in `cordial.toml` replaces the defaults, it does
    /// not union with them.
    #[serde(default = "default_stdio_skip_folders")]
    skip_folders: Vec<String>,
}

#[instrument(level = "debug")]
fn default_stdio_skip_folders() -> Vec<String> {
    vec!["tests/fixtures".to_string(), "tests/parity".to_string()]
}

impl TracingStdioPolicy {
    /// Whether `file` lives under a configured skip folder.
    #[instrument(level = "trace", skip(self, file))]
    pub fn skips_file(&self, file: &Path, crate_root: &Path) -> bool {
        let rel = file.strip_prefix(crate_root).unwrap_or(file);
        self.skip_folders.iter().any(|folder| {
            let prefix = Path::new(folder);
            rel == prefix || rel.starts_with(prefix)
        })
    }
}

impl Default for TracingStdioPolicy {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            println: true,
            eprintln: true,
            print: true,
            eprint: true,
            dbg: true,
            skip_cargo_protocol: true,
            skip_folders: default_stdio_skip_folders(),
        }
    }
}

impl Default for TracingThresholds {
    #[instrument(level = "debug", ret)]
    fn default() -> Self {
        Self {
            extra_skip: Vec::new(),
            apply_gate_crates: std::collections::HashMap::new(),
            apply_skip_crates: Vec::new(),
            subscriber: TracingSubscriberPolicy::default(),
            boundary: TracingBoundaryPolicy::default(),
            stdio: TracingStdioPolicy::default(),
            enabled: true,
        }
    }
}
