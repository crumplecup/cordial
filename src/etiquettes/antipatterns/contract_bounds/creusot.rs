//! Creusot `#[requires]`/`#[ensures]` unnamed-bound scan.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, ImplItemFn, ItemFn, ItemMacro, Meta};
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::etiquettes::antipatterns::types::AntipatternSiteRecord;
use crate::loader::{SharedParsedFile, module_path_from_src_file};

use super::ContractClause;
use super::index::{ContractIndex, is_trivial, normalize_tokens};

use super::{harness_macro_item_fn, make_finding, site_context};

#[instrument(level = "debug", skip(source, file, index), err(level = "warn"))]
pub(super) fn scan_creusot_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    index: &ContractIndex,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let parsed = crate::SourceLoadView::parse_fixture(file, source)?;
    Ok(collect_creusot_syntax(parsed.syntax(), file, src_root)?
        .iter()
        .filter_map(|clause| clause.finding(index))
        .collect())
}

#[instrument(level = "debug", skip(parsed), err(level = "warn"))]
pub(super) fn collect_creusot_shared(
    parsed: &SharedParsedFile,
    src_root: &Path,
) -> CordialResult<Vec<ContractClause>> {
    collect_creusot_syntax(parsed.syntax(), parsed.path(), src_root)
}

#[instrument(level = "debug", skip(syntax, file), err(level = "warn"))]
fn collect_creusot_syntax(
    syntax: &syn::File,
    file: &Path,
    src_root: &Path,
) -> CordialResult<Vec<ContractClause>> {
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = CreusotVisitor {
        file: file.to_path_buf(),
        module_prefix,
        fn_stack: Vec::new(),
        findings: Vec::new(),
        error: None,
    };
    visitor.visit_file(syntax);
    if let Some(err) = visitor.error {
        return Err(err);
    }
    Ok(visitor.findings)
}

struct CreusotVisitor {
    file: PathBuf,
    module_prefix: Vec<String>,
    fn_stack: Vec<String>,
    findings: Vec<ContractClause>,
    error: Option<CordialError>,
}

impl CreusotVisitor {
    #[instrument(level = "debug", skip(self, attrs))]
    fn check_attrs(&mut self, attrs: &[Attribute]) {
        for attr in attrs {
            let kind = if attr.path().is_ident("requires") {
                "requires"
            } else if attr.path().is_ident("ensures") {
                "ensures"
            } else {
                continue;
            };
            let Meta::List(list) = &attr.meta else {
                continue;
            };
            let normalized = normalize_tokens(list.tokens.clone());
            if is_trivial(&normalized) {
                continue;
            }
            let context = site_context(&self.module_prefix, &self.fn_stack.join("::"));
            if self.error.is_some() {
                return;
            }
            match make_finding(
                context,
                &self.file,
                attr.span().start().line as u32,
                &normalized,
            ) {
                Ok(record) => self.findings.push(ContractClause::collect(
                    record,
                    "creusot",
                    kind,
                    list.tokens.clone(),
                    Vec::new(),
                )),
                Err(error) => self.error = Some(error),
            }
        }
    }
}

impl<'ast> Visit<'ast> for CreusotVisitor {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_attrs(&node.attrs);
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_attrs(&node.attrs);
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_macro(&mut self, node: &'ast ItemMacro) {
        if let Some(item_fn) = harness_macro_item_fn(node) {
            self.visit_item_fn(&item_fn);
        }
        syn::visit::visit_item_macro(self, node);
    }
}
