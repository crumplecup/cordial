//! Walk a parsed file and emit antipattern site records.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use syn::visit::Visit;
use syn::{
    Fields, ItemEnum, ItemFn, ItemImpl, ItemMod, ItemStruct, ItemTrait, ItemType, TraitItem, Type,
};

use crate::config::{ErrorFieldStrategy, StaticRefStrategy};
use crate::enricher::is_cfg_test;

use super::preds::{
    cfg_sibling_real_param_names_in_impl_items, cfg_sibling_real_param_names_in_items, type_label,
};
use crate::error::CordialResult;
use crate::etiquettes::antipatterns::types::{AntipatternRuleId, AntipatternSiteRecord};

use tracing::instrument;
mod checks;
mod error_types;
mod trait_names;
pub(super) use error_types::{collect_error_kind_names, collect_error_type_names};
pub(super) use trait_names::collect_local_trait_names;
use trait_names::is_foreign_trait_impl;
pub(super) struct AntipatternScanVisitor<'a> {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    impl_type: Option<String>,
    fn_stack: Vec<String>,
    in_trait_definition: bool,
    in_foreign_trait_impl: bool,
    local_trait_names: &'a HashSet<String>,
    error_type_names: &'a HashSet<String>,
    const_placed_types: &'a HashSet<String>,
    static_ref_strategy: StaticRefStrategy,
    error_field_strategy: ErrorFieldStrategy,
    /// `[antipatterns] skip_paths`: rule id to crate-relative path
    /// prefixes exempt from that rule. See [`path_is_skipped`].
    skip_paths: &'a HashMap<String, Vec<String>>,
    cfg_sibling_real_params: HashMap<String, HashSet<String>>,
    findings: Vec<AntipatternSiteRecord>,
    error: Option<crate::error::CordialError>,
}

impl<'a> AntipatternScanVisitor<'a> {
    #[instrument(
        level = "debug",
        skip(
            file,
            local_trait_names,
            error_type_names,
            const_placed_types,
            static_ref_strategy,
            error_field_strategy,
            skip_paths
        )
    )]
    pub(super) fn new(
        file: PathBuf,
        crate_root: PathBuf,
        module_prefix: Vec<String>,
        local_trait_names: &'a HashSet<String>,
        error_type_names: &'a HashSet<String>,
        const_placed_types: &'a HashSet<String>,
        static_ref_strategy: StaticRefStrategy,
        error_field_strategy: ErrorFieldStrategy,
        skip_paths: &'a HashMap<String, Vec<String>>,
    ) -> Self {
        Self {
            file,
            crate_root,
            module_prefix,
            impl_type: None,
            fn_stack: Vec::new(),
            in_trait_definition: false,
            in_foreign_trait_impl: false,
            local_trait_names,
            error_type_names,
            const_placed_types,
            static_ref_strategy,
            error_field_strategy,
            skip_paths,
            cfg_sibling_real_params: HashMap::new(),
            findings: Vec::new(),
            error: None,
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn into_findings(self) -> CordialResult<Vec<AntipatternSiteRecord>> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(self.findings)
    }
}

impl AntipatternScanVisitor<'_> {
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

    #[instrument(level = "debug", skip(self, rule_id))]
    fn push_record(
        &mut self,
        rule_id: AntipatternRuleId,
        context: String,
        line: u32,
        snippet: String,
    ) {
        if self.error.is_some() {
            return;
        }
        let rel_file = self.rel_file();
        if path_is_skipped(self.skip_paths, rule_id, &rel_file) {
            return;
        }
        match AntipatternSiteRecord::builder()
            .rule_id(rule_id)
            .context(context)
            .file(rel_file)
            .line(line)
            .snippet(snippet)
            .build()
        {
            Ok(record) => self.findings.push(record),
            Err(error) => self.error = Some(error),
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn rel_file(&self) -> PathBuf {
        self.file
            .strip_prefix(&self.crate_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| self.file.clone())
    }

    #[instrument(level = "debug", skip(self, items))]
    fn visit_module_items(&mut self, items: &[syn::Item], module_prefix: &[String]) {
        let prev_prefix = self.module_prefix.clone();
        self.module_prefix = module_prefix.to_vec();
        let prev_siblings = std::mem::replace(
            &mut self.cfg_sibling_real_params,
            cfg_sibling_real_param_names_in_items(items),
        );
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.cfg_sibling_real_params = prev_siblings;
        self.module_prefix = prev_prefix;
    }

    #[instrument(level = "debug", skip(self, item_mod))]
    fn visit_mod(&mut self, item_mod: &ItemMod) {
        if is_cfg_test(&item_mod.attrs) {
            return;
        }
        let Some((_, items)) = &item_mod.content else {
            return;
        };
        let mut nested = self.module_prefix.clone();
        nested.push(item_mod.ident.to_string());
        self.visit_module_items(items, &nested);
    }
}

impl<'ast> Visit<'ast> for AntipatternScanVisitor<'_> {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_file(&mut self, node: &'ast syn::File) {
        let module_prefix = self.module_prefix.clone();
        self.visit_module_items(&node.items, &module_prefix);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        let owner = node.ident.to_string();
        match &node.fields {
            Fields::Named(fields) => self.check_named_fields(&owner, fields),
            Fields::Unnamed(fields) => self.check_unnamed_fields(&owner, fields),
            Fields::Unit => {}
        }
        syn::visit::visit_item_struct(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        let enum_name = node.ident.to_string();
        for variant in &node.variants {
            self.check_enum_variant(&enum_name, variant);
        }
        syn::visit::visit_item_enum(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_fn_sig(&node.attrs, &node.sig, &node.block);
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        let prev = self.in_trait_definition;
        self.in_trait_definition = true;
        for item in &node.items {
            if matches!(item, TraitItem::Fn(_)) {
                continue;
            }
            syn::visit::visit_trait_item(self, item);
        }
        self.in_trait_definition = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_type(&mut self, node: &'ast ItemType) {
        self.fn_stack.push(node.ident.to_string());
        syn::visit::visit_item_type(self, node);
        self.fn_stack.pop();
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let prev_type = self.impl_type.clone();
        let prev_foreign = self.in_foreign_trait_impl;
        let prev_siblings = std::mem::replace(
            &mut self.cfg_sibling_real_params,
            cfg_sibling_real_param_names_in_impl_items(&node.items),
        );
        self.impl_type = Some(type_label(&node.self_ty));
        self.in_foreign_trait_impl = is_foreign_trait_impl(node, self.local_trait_names);
        syn::visit::visit_item_impl(self, node);
        self.impl_type = prev_type;
        self.in_foreign_trait_impl = prev_foreign;
        self.cfg_sibling_real_params = prev_siblings;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_fn_sig(&node.attrs, &node.sig, &node.block);
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(level = "debug", skip(self, _node))]
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}

    #[instrument(level = "debug", skip(self, node))]
    fn visit_type(&mut self, node: &'ast Type) {
        self.check_box_dyn_error_type(node);
        self.check_string_error_type(node);
        syn::visit::visit_type(self, node);
    }
}

/// Whether `rel_file` is exempt from `rule_id` under `[antipatterns]
/// skip_paths`. Matched as an exact file or a path prefix -- the same
/// folder-or-file idiom `ModularityThresholds::is_generated_file` and
/// `mod_thin_skip_matches` already use.
#[instrument(level = "debug", skip(skip_paths, rule_id), ret)]
fn path_is_skipped(
    skip_paths: &HashMap<String, Vec<String>>,
    rule_id: AntipatternRuleId,
    rel_file: &Path,
) -> bool {
    let Some(entries) = skip_paths.get(rule_id.as_str()) else {
        return false;
    };
    entries.iter().any(|entry| {
        let prefix = Path::new(entry);
        rel_file == prefix || rel_file.starts_with(prefix)
    })
}
