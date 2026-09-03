//! syn-based scan for leftover stdio macros (`println!`, `print!`, `dbg!`, …).

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{File, ItemMod};

use crate::config::TracingStdioPolicy;
use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, quality_scan_trees};

use super::types::{PrintRuleId, PrintSiteRecord};

use tracing::instrument;

/// Scan `src/` and `tests/` for leftover std print macros.
#[instrument(level = "debug", skip(policy), err(level = "warn"))]
pub fn scan_crate_tracing_print(
    crate_root: &Path,
    policy: &TracingStdioPolicy,
) -> CordialResult<Vec<PrintSiteRecord>> {
    let mut findings = Vec::new();
    for tree_root in quality_scan_trees(crate_root) {
        findings.extend(scan_source_tree(&tree_root, crate_root, policy)?);
    }
    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.snippet().cmp(b.snippet()))
    });
    Ok(findings)
}

#[instrument(level = "debug", skip(policy), err(level = "warn"))]
pub fn scan_source_tree(
    tree_root: &Path,
    crate_root: &Path,
    policy: &TracingStdioPolicy,
) -> CordialResult<Vec<PrintSiteRecord>> {
    let mut findings = Vec::new();
    if !tree_root.is_dir() {
        return Ok(findings);
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
        if policy.skips_file(path, crate_root) {
            continue;
        }
        let source = std::fs::read_to_string(path)?;
        findings.extend(scan_rust_source(
            &source, path, tree_root, crate_root, policy,
        )?);
    }

    Ok(findings)
}

/// Scan one Rust source file and return records.
#[instrument(level = "debug", skip(source, file, policy), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
    policy: &TracingStdioPolicy,
) -> CordialResult<Vec<PrintSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(tree_root, file);
    scan_syntax(&syntax, file, crate_root, &module_prefix, policy)
}

#[instrument(
    level = "debug",
    skip(syntax, file, crate_root, module_prefix, policy),
    err(level = "warn")
)]
fn scan_syntax(
    syntax: &File,
    file: &Path,
    crate_root: &Path,
    module_prefix: &[String],
    policy: &TracingStdioPolicy,
) -> CordialResult<Vec<PrintSiteRecord>> {
    let mut visitor = PrintVisitor {
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix: module_prefix.to_vec(),
        policy: policy.clone(),
        findings: Vec::new(),
        error: None,
    };
    visitor.visit_file(syntax);
    if let Some(error) = visitor.error {
        return Err(error);
    }
    Ok(visitor.findings)
}

struct PrintVisitor {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    policy: TracingStdioPolicy,
    findings: Vec<PrintSiteRecord>,
    error: Option<crate::error::CordialError>,
}

impl PrintVisitor {
    #[instrument(level = "debug", skip(self))]
    fn site_context(&self) -> String {
        if self.module_prefix.is_empty() {
            "<crate>".to_string()
        } else {
            self.module_prefix.join("::")
        }
    }

    #[instrument(level = "debug", skip(self))]
    fn rule_enabled(&self, rule: PrintRuleId) -> bool {
        match rule {
            PrintRuleId::Println => self.policy.println(),
            PrintRuleId::Eprintln => self.policy.eprintln(),
            PrintRuleId::Print => self.policy.print(),
            PrintRuleId::Eprint => self.policy.eprint(),
            PrintRuleId::Dbg => self.policy.dbg(),
        }
    }
}

impl<'ast> Visit<'ast> for PrintVisitor {
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
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if let Some(rule_id) = std_print_rule(&node.path) {
            if self.policy.skip_cargo_protocol() && is_cargo_protocol(&node.tokens) {
                syn::visit::visit_macro(self, node);
                return;
            }
            if !self.rule_enabled(rule_id) {
                syn::visit::visit_macro(self, node);
                return;
            }
            let mut file = self.file.clone();
            if let Ok(rel) = file.strip_prefix(&self.crate_root) {
                file = rel.to_path_buf();
            }
            if self.error.is_some() {
                syn::visit::visit_macro(self, node);
                return;
            }
            match PrintSiteRecord::builder()
                .rule_id(rule_id)
                .context(self.site_context())
                .file(file)
                .line(node.path.span().start().line as u32)
                .snippet(rule_id.snippet().to_string())
                .build()
            {
                Ok(record) => self.findings.push(record),
                Err(error) => self.error = Some(error),
            }
        }
        syn::visit::visit_macro(self, node);
    }
}

/// Last path segment is a leftover stdio macro (`std::println!` included).
#[instrument(level = "trace", skip(path), ret)]
fn std_print_rule(path: &syn::Path) -> Option<PrintRuleId> {
    let ident = path.segments.last()?.ident.to_string();
    match ident.as_str() {
        "println" => Some(PrintRuleId::Println),
        "eprintln" => Some(PrintRuleId::Eprintln),
        "print" => Some(PrintRuleId::Print),
        "eprint" => Some(PrintRuleId::Eprint),
        "dbg" => Some(PrintRuleId::Dbg),
        _ => None,
    }
}

/// `println!("cargo:…")` / `println!("cargo::…")` is build-script protocol.
#[instrument(level = "trace", skip(tokens), ret)]
fn is_cargo_protocol(tokens: &proc_macro2::TokenStream) -> bool {
    first_string_literal(&tokens.to_string())
        .is_some_and(|value| value.starts_with("cargo:") || value.starts_with("cargo::"))
}

#[instrument(level = "trace", skip(tokens), ret)]
fn first_string_literal(tokens: &str) -> Option<String> {
    let start = tokens.find('"')?;
    let rest = &tokens[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
