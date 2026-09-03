//! syn-based scan for the binary error-boundary policy: a fallible
//! `fn main` must convert its error to a tracing warn/error emission
//! before the process boundary, not let it bubble to a crash.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, File, ImplItemFn, ItemFn, ItemMod};

use crate::config::TracingBoundaryPolicy;
use crate::error::CordialResult;
use crate::loader::{path_has_fixtures, quality_scan_trees};

use super::detect::BoundaryBodyFacts;
use super::types::{BoundaryRuleId, BoundarySiteRecord};

use tracing::instrument;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Lib,
    Bin,
    Test,
}

#[derive(Debug, Clone)]
struct FnSite {
    name: String,
    is_main: bool,
    file: PathBuf,
    line: u32,
    facts: BoundaryBodyFacts,
    context: String,
}

/// Scan lib, bins, and `tests/` for the binary error-boundary policy.
/// Every function in the crate is scanned (not just `fn main`) so a
/// helper `main` delegates to (e.g. `Cli::act`) can be recognized as
/// already reporting its own errors — mirrors
/// [`super::super::subscriber::scan_crate_tracing_subscriber`]'s
/// helper-name delegation strategy.
#[instrument(level = "debug", skip(policy), err(level = "warn"))]
pub fn scan_crate_tracing_boundary(
    crate_root: &Path,
    crate_name: &str,
    policy: &TracingBoundaryPolicy,
    skip_program_lints: bool,
) -> CordialResult<Vec<BoundarySiteRecord>> {
    let mut sites = Vec::new();
    for tree_root in quality_scan_trees(crate_root) {
        sites.extend(collect_tree(
            &tree_root,
            crate_root,
            crate_name,
            policy.known_helper_paths(),
        )?);
    }

    let has_bin = crate_root.join("src").join("main.rs").is_file()
        || crate_root.join("src").join("bin").is_dir();

    let safe_names: Vec<&str> = sites
        .iter()
        .filter(|site| site.facts.is_fallible && site.facts.reports_errors())
        .map(|site| site.name.as_str())
        .collect();

    let mut findings = Vec::new();
    for site in &sites {
        if policy.main_reports_errors()
            && !skip_program_lints
            && has_bin
            && site.is_main
            && site.facts.is_fallible
            && !site.facts.reports_errors()
            && !site.facts.calls_safe_helper(&safe_names)
        {
            findings.push(record(
                site,
                "fallible fn main never converts its error to a tracing warn/error emission \
                 before returning — add #[instrument(err(...))] or emit tracing::warn!/error! \
                 on the error path",
            )?);
        }
    }

    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.rule_id().as_str().cmp(b.rule_id().as_str()))
    });
    Ok(findings)
}

#[instrument(level = "debug", skip(site), err(level = "warn"))]
fn record(site: &FnSite, snippet: &str) -> CordialResult<BoundarySiteRecord> {
    BoundarySiteRecord::builder()
        .rule_id(BoundaryRuleId::MainSilent)
        .context(site.context.clone())
        .file(site.file.clone())
        .line(site.line)
        .snippet(snippet.to_string())
        .build()
}

#[instrument(level = "debug", skip(known_helper_paths), err(level = "warn"))]
fn collect_tree(
    tree_root: &Path,
    crate_root: &Path,
    crate_name: &str,
    known_helper_paths: &[String],
) -> CordialResult<Vec<FnSite>> {
    let mut sites = Vec::new();
    if !tree_root.is_dir() {
        return Ok(sites);
    }
    for entry in walkdir::WalkDir::new(tree_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        if path_has_fixtures(path, crate_root) {
            continue;
        }
        let source = std::fs::read_to_string(path)?;
        sites.extend(scan_file(
            &source,
            path,
            crate_root,
            crate_name,
            known_helper_paths,
        )?);
    }
    Ok(sites)
}

#[instrument(level = "debug", skip(source, known_helper_paths), err(level = "warn"))]
fn scan_file(
    source: &str,
    file: &Path,
    crate_root: &Path,
    crate_name: &str,
    known_helper_paths: &[String],
) -> CordialResult<Vec<FnSite>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let kind = file_kind(file, crate_root);
    let mut relative = file.to_path_buf();
    if let Ok(stripped) = file.strip_prefix(crate_root) {
        relative = stripped.to_path_buf();
    }
    let mut visitor = SiteVisitor {
        crate_name: crate_name.to_string(),
        file: relative,
        kind,
        module_prefix: Vec::new(),
        sites: Vec::new(),
        known_helper_paths: known_helper_paths.to_vec(),
    };
    visitor.visit_file(&syntax);
    Ok(visitor.sites)
}

#[instrument(level = "debug")]
fn file_kind(path: &Path, crate_root: &Path) -> FileKind {
    let relative = path.strip_prefix(crate_root).unwrap_or(path);
    let components: Vec<_> = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();
    if components.first().copied() == Some("tests") {
        return FileKind::Test;
    }
    if components == ["src", "main.rs"] {
        return FileKind::Bin;
    }
    if components.first().copied() == Some("src") && components.get(1).copied() == Some("bin") {
        return FileKind::Bin;
    }
    FileKind::Lib
}

struct SiteVisitor {
    crate_name: String,
    file: PathBuf,
    kind: FileKind,
    module_prefix: Vec<String>,
    sites: Vec<FnSite>,
    known_helper_paths: Vec<String>,
}

impl SiteVisitor {
    #[instrument(level = "debug", skip(self))]
    fn context(&self, name: &str) -> String {
        if self.module_prefix.is_empty() {
            format!("{}::{name}", self.crate_name)
        } else {
            format!(
                "{}::{}::{name}",
                self.crate_name,
                self.module_prefix.join("::")
            )
        }
    }

    #[instrument(level = "debug", skip(self, sig, attrs, block))]
    fn push_fn(
        &mut self,
        name: &str,
        sig: &syn::Signature,
        attrs: &[Attribute],
        block: &syn::Block,
        line: u32,
    ) {
        self.sites.push(FnSite {
            name: name.to_string(),
            is_main: name == "main" && self.kind == FileKind::Bin,
            file: self.file.clone(),
            line,
            facts: BoundaryBodyFacts::from_fn(sig, attrs, block, &self.known_helper_paths),
            context: self.context(name),
        });
    }
}

impl<'ast> Visit<'ast> for SiteVisitor {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_file(&mut self, node: &'ast File) {
        syn::visit::visit_file(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        let Some((_, items)) = &node.content else {
            syn::visit::visit_item_mod(self, node);
            return;
        };
        let prev = self.module_prefix.clone();
        self.module_prefix.push(node.ident.to_string());
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.module_prefix = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.push_fn(
            &node.sig.ident.to_string(),
            &node.sig,
            &node.attrs,
            &node.block,
            node.span().start().line as u32,
        );
        syn::visit::visit_item_fn(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.push_fn(
            &node.sig.ident.to_string(),
            &node.sig,
            &node.attrs,
            &node.block,
            node.span().start().line as u32,
        );
        syn::visit::visit_impl_item_fn(self, node);
    }
}
