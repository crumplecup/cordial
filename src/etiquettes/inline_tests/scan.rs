//! syn-based scan for tests mixed into `src/`.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, File, ImplItemFn, ItemFn, ItemImpl, ItemMod, ItemUse, Meta};

use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, path_has_fixtures};

use super::types::{InlineTestRuleId, InlineTestSiteRecord};

use tracing::instrument;

/// Scan one crate for inline tests.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_inline_tests(crate_root: &Path) -> CordialResult<Vec<InlineTestSiteRecord>> {
    scan_source_tree(&crate_root.join("src"), crate_root)
}

#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_source_tree(
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<InlineTestSiteRecord>> {
    let mut findings = Vec::new();
    if !src_root.is_dir() {
        return Ok(findings);
    }

    for entry in walkdir::WalkDir::new(src_root)
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
        findings.extend(scan_rust_source(&source, path, src_root, crate_root)?);
    }

    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.snippet().cmp(b.snippet()))
    });

    Ok(findings)
}

/// Scan one Rust source file and return records.
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<InlineTestSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = InlineTestVisitor {
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        cfg_test_depth: 0,
        findings: Vec::new(),
        error: None,
    };
    visitor.visit_file(&syntax);
    if let Some(error) = visitor.error {
        return Err(error);
    }
    Ok(visitor.findings)
}

struct InlineTestVisitor {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    cfg_test_depth: usize,
    findings: Vec<InlineTestSiteRecord>,
    error: Option<crate::error::CordialError>,
}

impl InlineTestVisitor {
    #[instrument(level = "debug", skip(self))]
    fn site_context(&self) -> String {
        if self.module_prefix.is_empty() {
            "<crate>".to_string()
        } else {
            self.module_prefix.join("::")
        }
    }

    #[instrument(level = "debug", skip(self, rule_id, snippet, attr))]
    fn push(&mut self, rule_id: InlineTestRuleId, snippet: String, attr: &Attribute) {
        let mut file = self.file.clone();
        if let Ok(rel) = file.strip_prefix(&self.crate_root) {
            file = rel.to_path_buf();
        }
        if self.error.is_some() {
            return;
        }
        match InlineTestSiteRecord::builder()
            .rule_id(rule_id)
            .context(self.site_context())
            .file(file)
            .line(attr.span().start().line as u32)
            .snippet(snippet)
            .build()
        {
            Ok(record) => self.findings.push(record),
            Err(error) => self.error = Some(error),
        }
    }

    #[instrument(level = "debug", skip(self, attrs, ident))]
    fn maybe_cfg_item(&mut self, attrs: &[Attribute], kind: &str, ident: &str) -> bool {
        if self.cfg_test_depth > 0 {
            return false;
        }
        if !is_cfg_test(attrs) {
            return false;
        }
        let Some(attr) = cfg_test_attr(attrs) else {
            return false;
        };
        self.push(
            InlineTestRuleId::Cfg001,
            format!("#[cfg(test)] {kind} {ident}"),
            attr,
        );
        true
    }
}

impl<'ast> Visit<'ast> for InlineTestVisitor {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_file(&mut self, node: &'ast File) {
        syn::visit::visit_file(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        let prev_prefix = self.module_prefix.clone();
        self.module_prefix.push(node.ident.to_string());
        let cfg_test = is_cfg_test(&node.attrs);
        if cfg_test
            && self.cfg_test_depth == 0
            && let Some(attr) = cfg_test_attr(&node.attrs)
        {
            self.push(
                InlineTestRuleId::Mod001,
                format!("#[cfg(test)] mod {}", node.ident),
                attr,
            );
        }
        if cfg_test {
            self.cfg_test_depth += 1;
        }
        syn::visit::visit_item_mod(self, node);
        if cfg_test {
            self.cfg_test_depth -= 1;
        }
        self.module_prefix = prev_prefix;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let ident = node.sig.ident.to_string();
        self.module_prefix.push(ident.clone());
        let flagged_cfg = self.maybe_cfg_item(&node.attrs, "fn", &ident);
        if !flagged_cfg
            && self.cfg_test_depth == 0
            && let Some(attr) = test_fn_attr(&node.attrs)
        {
            self.push(InlineTestRuleId::Fn001, format!("#[test] fn {ident}"), attr);
        }
        syn::visit::visit_item_fn(self, node);
        self.module_prefix.pop();
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        let ident = node.sig.ident.to_string();
        let flagged_cfg = self.maybe_cfg_item(&node.attrs, "fn", &ident);
        if !flagged_cfg
            && self.cfg_test_depth == 0
            && let Some(attr) = test_fn_attr(&node.attrs)
        {
            self.push(InlineTestRuleId::Fn001, format!("#[test] fn {ident}"), attr);
        }
        syn::visit::visit_impl_item_fn(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let _ = self.maybe_cfg_item(&node.attrs, "impl", "<impl>");
        let nested = is_cfg_test(&node.attrs);
        if nested {
            self.cfg_test_depth += 1;
        }
        syn::visit::visit_item_impl(self, node);
        if nested {
            self.cfg_test_depth -= 1;
        }
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        let _ = self.maybe_cfg_item(&node.attrs, "use", "<use>");
        syn::visit::visit_item_use(self, node);
    }
}

#[instrument(level = "trace", skip(attrs))]
fn is_cfg_test(attrs: &[Attribute]) -> bool {
    cfg_test_attr(attrs).is_some()
}

#[instrument(level = "trace", skip(attrs))]
fn cfg_test_attr(attrs: &[Attribute]) -> Option<&Attribute> {
    attrs.iter().find(|attr| {
        let Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        list.tokens.to_string().replace(' ', "") == "test"
    })
}

#[instrument(level = "trace", skip(attrs))]
fn test_fn_attr(attrs: &[Attribute]) -> Option<&Attribute> {
    attrs.iter().find(|attr| {
        let segments: Vec<_> = attr
            .path()
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        match segments.as_slice() {
            [name] if name == "test" || name == "rstest" => true,
            [_, name] if name == "test" => true,
            _ => false,
        }
    })
}
