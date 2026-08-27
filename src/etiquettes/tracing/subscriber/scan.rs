//! syn-based scan for tracing-subscriber init policy.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, File, ImplItemFn, ItemFn, ItemMod};

use crate::config::TracingSubscriberPolicy;
use crate::error::CordialResult;
use crate::loader::{path_has_fixtures, quality_scan_trees};

use super::detect::InitBodyFacts;
use super::types::{SubscriberRuleId, SubscriberSiteRecord};

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
    kind: FileKind,
    is_main: bool,
    is_test: bool,
    file: PathBuf,
    line: u32,
    facts: InitBodyFacts,
    context: String,
}

/// Scan lib, bins, and `tests/` for subscriber-init policy rows.
#[instrument(level = "debug", skip(policy), err(level = "warn"))]
pub fn scan_crate_tracing_subscriber(
    crate_root: &Path,
    crate_name: &str,
    policy: &TracingSubscriberPolicy,
    skip_program_lints: bool,
) -> CordialResult<Vec<SubscriberSiteRecord>> {
    let mut sites = Vec::new();
    for tree_root in quality_scan_trees(crate_root) {
        sites.extend(collect_tree(
            &tree_root,
            crate_root,
            crate_name,
            policy.known_helper_paths(),
        )?);
    }

    let has_lib = crate_root.join("src").join("lib.rs").is_file();
    let has_bin = crate_root.join("src").join("main.rs").is_file()
        || crate_root.join("src").join("bin").is_dir();

    let helper_names: Vec<&str> = sites
        .iter()
        .filter(|site| site.facts.calls_install)
        .map(|site| site.name.as_str())
        .collect();

    let mut findings = Vec::new();
    for site in &sites {
        if policy.init_in_main()
            && !skip_program_lints
            && has_bin
            && site.is_main
            && !site.facts.installs_or_delegates()
            && !site.facts.calls_helper(&helper_names)
        {
            findings.push(record(
                SubscriberRuleId::Main,
                site,
                "fn main never installs a tracing subscriber — call the library helper",
            ));
        }
        if policy.init_in_tests()
            && !skip_program_lints
            && site.is_test
            && !site.facts.installs_or_delegates()
            && !site.facts.calls_helper(&helper_names)
        {
            findings.push(record(
                SubscriberRuleId::Test,
                site,
                "#[test] never installs a tracing subscriber — call the library helper",
            ));
        }
        if policy.helper_in_lib()
            && has_lib
            && site.facts.calls_install
            && matches!(site.kind, FileKind::Bin | FileKind::Test)
        {
            findings.push(record(
                SubscriberRuleId::Lib,
                site,
                "subscriber init lives outside the library — move it to one documented helper",
            ));
        }
        if policy.rust_log_fallback() && site.facts.calls_install && !site.facts.rust_log_ok() {
            findings.push(record(
                SubscriberRuleId::RustLog,
                site,
                "init helper must read RUST_LOG with a fallback (try_from_default_env + unwrap_or)",
            ));
        }
        if policy.idempotent() && site.facts.calls_install && !site.facts.idempotent_ok() {
            findings.push(record(
                SubscriberRuleId::Idempotent,
                site,
                "init helper uses init() without Once/OnceLock — use try_init() or wrap in Once",
            ));
        }
    }

    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.rule_id.as_str().cmp(b.rule_id.as_str()))
            .then(a.snippet.cmp(&b.snippet))
    });
    Ok(findings)
}

#[instrument(level = "debug", skip(site))]
fn record(rule_id: SubscriberRuleId, site: &FnSite, snippet: &str) -> SubscriberSiteRecord {
    SubscriberSiteRecord {
        rule_id,
        context: site.context.clone(),
        file: site.file.clone(),
        line: site.line,
        snippet: snippet.to_string(),
    }
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

    #[instrument(level = "debug", skip(self, attrs, block))]
    fn push_fn(&mut self, name: &str, attrs: &[Attribute], block: &syn::Block, line: u32) {
        self.sites.push(FnSite {
            name: name.to_string(),
            kind: self.kind,
            is_main: name == "main" && self.kind == FileKind::Bin,
            is_test: self.kind == FileKind::Test && is_test_fn(attrs),
            file: self.file.clone(),
            line,
            facts: InitBodyFacts::from_block(block, &self.known_helper_paths),
            context: self.context(name),
        });
    }
}

#[instrument(level = "debug", skip(attrs))]
fn is_test_fn(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "test")
    })
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
            &node.attrs,
            &node.block,
            node.span().start().line as u32,
        );
        syn::visit::visit_impl_item_fn(self, node);
    }
}
