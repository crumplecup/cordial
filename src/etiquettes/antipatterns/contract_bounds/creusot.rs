//! Creusot `#[requires]`/`#[ensures]` unnamed-bound scan.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, ImplItemFn, ItemFn, ItemMacro, Meta};
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::etiquettes::antipatterns::types::AntipatternSiteRecord;
use crate::loader::module_path_from_src_file;

use super::index::{ContractIndex, is_trivial, normalize_tokens};
use super::{harness_macro_item_fn, make_finding, site_context};

#[instrument(level = "debug", skip(source, file, index), err(level = "warn"))]
pub(super) fn scan_creusot_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
    index: &ContractIndex,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = CreusotVisitor {
        crate_name: crate_name.to_string(),
        file: file.to_path_buf(),
        module_prefix,
        fn_stack: Vec::new(),
        index,
        findings: Vec::new(),
    };
    visitor.visit_file(&syntax);
    Ok(visitor.findings)
}

struct CreusotVisitor<'a> {
    crate_name: String,
    file: PathBuf,
    module_prefix: Vec<String>,
    fn_stack: Vec<String>,
    index: &'a ContractIndex,
    findings: Vec<AntipatternSiteRecord>,
}

impl CreusotVisitor<'_> {
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
            if is_trivial(&normalized)
                || self
                    .index
                    .matches_named_call("creusot", kind, list.tokens.clone())
            {
                continue;
            }
            let context = site_context(&self.module_prefix, &self.fn_stack.join("::"));
            self.findings.push(make_finding(
                &self.crate_name,
                context,
                &self.file,
                attr.span().start().line as u32,
                &normalized,
            ));
        }
    }
}

impl<'ast> Visit<'ast> for CreusotVisitor<'_> {
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
