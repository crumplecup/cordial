//! syn-based scan for panic, unreachable, expect, unwrap, and compile_error sites.

mod embedded;
mod snippets;
mod source;
#[cfg(feature = "verus_ir")]
mod verus_panics;
#[cfg(not(feature = "verus_ir"))]
mod verus_recovery;
mod visit;

use std::path::{Path, PathBuf};

use syn::visit::Visit;

use super::kani_reach::{KaniReachability, build_kani_reachability};
use super::types::{PanicKind, PanicSiteRecord};
use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, quality_scan_trees};

pub(super) use self::snippets::has_cfg_flag;
use self::source::{ParsedFile, parse_source_tree};

use tracing::instrument;
/// Scan `src/` and `tests/` under `crate_root`, excluding `fixtures/` paths.
///
/// Parses every file in the crate once up front to build a
/// `#[kani::proof]` reachability closure (see `kani_reach`'s own doc
/// comment) before scanning any of them for panic sites, since a site
/// that's part of a proof's own failure mechanism must never be flagged
/// regardless of which file it's found in relative to which file
/// declares the harness that reaches it.
///
/// String literals that themselves parse as Rust and contain abort
/// macros or `.unwrap` / `.expect` are scanned too: an inline fixture
/// program is still an abort site until it lives under `tests/fixtures/`
/// or `tests/parity/` (the path skip above). Assertion messages like
/// `"panic!"` do not parse as a file and are not flagged.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_panics(crate_root: &Path) -> CordialResult<Vec<PanicSiteRecord>> {
    let mut parsed = Vec::new();
    for tree_root in quality_scan_trees(crate_root) {
        parsed.extend(parse_source_tree(&tree_root, crate_root)?);
    }
    let reachability = build_kani_reachability(parsed.iter().map(ParsedFile::syntax));

    let mut findings = Vec::new();
    for file in &parsed {
        findings.extend(scan_parsed_file(file, crate_root, &reachability)?);
    }
    #[cfg(feature = "verus_ir")]
    {
        let verus_ir = crate::verus_ir::scan_crate_verus_ir(crate_root)?;
        findings.extend(verus_panics::findings(&verus_ir, crate_root)?);
    }
    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.context().cmp(b.context()))
            .then(a.snippet().cmp(b.snippet()))
    });
    Ok(findings)
}

/// Scan a crate `src` tree for panic sites.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_source_tree(src_root: &Path, crate_root: &Path) -> CordialResult<Vec<PanicSiteRecord>> {
    let parsed = parse_source_tree(src_root, crate_root)?;
    let reachability = build_kani_reachability(parsed.iter().map(ParsedFile::syntax));
    let mut findings = Vec::new();
    for file in &parsed {
        findings.extend(scan_parsed_file(file, crate_root, &reachability)?);
    }
    Ok(findings)
}

/// Scan one Rust source file and return records.
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<PanicSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let reachability = build_kani_reachability(std::iter::once(&syntax));
    let parsed = ParsedFile::new(file.to_path_buf(), src_root.to_path_buf(), syntax);
    let findings = scan_parsed_file(&parsed, crate_root, &reachability)?;
    #[cfg(feature = "verus_ir")]
    let findings = {
        let mut findings = findings;
        let module_path = module_path_from_src_file(src_root, file).join("::");
        let verus_ir = crate::verus_ir::scan_verus_rust_source(source, file, &module_path)?;
        findings.extend(verus_panics::findings(&verus_ir, crate_root)?);
        findings
    };
    Ok(findings)
}

#[instrument(level = "debug", skip(file, reachability), err(level = "warn"))]
fn scan_parsed_file(
    file: &ParsedFile,
    crate_root: &Path,
    reachability: &KaniReachability,
) -> CordialResult<Vec<PanicSiteRecord>> {
    let module_prefix = module_path_from_src_file(file.src_root(), file.path());
    let mut visitor = PanicScanVisitor {
        file: file.path().clone(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        impl_type: None,
        fn_stack: Vec::new(),
        in_cfg_test: false,
        in_cfg_not_kani: false,
        reachability,
        findings: Vec::new(),
        exempt_error_assertion_lines: std::collections::HashSet::new(),
        in_embedded_source: false,
        error: None,
    };
    visitor.visit_file(file.syntax());
    if let Some(error) = visitor.error {
        return Err(error);
    }
    Ok(visitor.findings)
}

struct PanicScanVisitor<'a> {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    impl_type: Option<String>,
    fn_stack: Vec<String>,
    in_cfg_test: bool,
    /// True inside a `#[cfg(not(kani))]` block -- overrides
    /// `reachability`, since that code never runs during Kani
    /// verification even when its enclosing function is otherwise
    /// reachable from a `#[kani::proof]` harness (see e.g.
    /// `symbolic_any`'s two-branch shape in amenable_kani::compose).
    in_cfg_not_kani: bool,
    reachability: &'a KaniReachability,
    findings: Vec<PanicSiteRecord>,
    /// Line numbers of `.expect_err(..)`/`.unwrap_err()` calls that
    /// assert a fact against the extracted error value rather than
    /// discarding/propagating it -- see `error_assertion`'s own doc
    /// comment. Accumulates across every `Block` visited in the file
    /// (line numbers are unique within one file), populated by
    /// `visit_block` before it descends into that block's own
    /// statements.
    exempt_error_assertion_lines: std::collections::HashSet<u32>,
    /// True while visiting a string literal that parsed as Rust. Nested
    /// strings inside that program are not scanned again.
    in_embedded_source: bool,
    error: Option<crate::error::CordialError>,
}

impl PanicScanVisitor<'_> {
    #[instrument(level = "debug", skip(self))]
    fn site_context(&self) -> String {
        let mut parts = self.module_prefix.clone();
        if let Some(ty) = &self.impl_type {
            parts.push(ty.clone());
        }
        parts.extend(self.fn_stack.iter().cloned());
        if parts.is_empty() {
            "<crate>".to_string()
        } else {
            parts.join("::")
        }
    }

    #[instrument(level = "debug", skip(self, kind))]
    fn push_finding(&mut self, kind: PanicKind, line: u32, snippet: String) {
        // A site only reachable from a #[kani::proof] harness (or nested
        // inside #[cfg(kani)]) is that harness's own failure mechanism --
        // Kani checks reachable panics/asserts, not a harness's return
        // value, so routing this through Result would silently disable
        // the check. See kani_reach's own doc comment.
        if !self.in_cfg_not_kani
            && self
                .fn_stack
                .last()
                .is_some_and(|key| self.reachability.contains(key))
        {
            return;
        }
        let mut file = self.file.clone();
        if let Ok(rel) = file.strip_prefix(&self.crate_root) {
            file = rel.to_path_buf();
        }
        if kind == PanicKind::Unwrap
            && self.findings.last().is_some_and(|last| {
                last.kind() == PanicKind::Unwrap && last.line() == line && last.file() == &file
            })
        {
            return;
        }
        if self.error.is_some() {
            return;
        }
        match PanicSiteRecord::builder()
            .kind(kind)
            .context(self.site_context())
            .file(file)
            .line(line)
            .snippet(snippet)
            .cfg_test(self.in_cfg_test)
            .build()
        {
            Ok(record) => self.findings.push(record),
            Err(error) => self.error = Some(error),
        }
    }
}
