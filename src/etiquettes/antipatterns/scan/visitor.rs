//! Walk a parsed file and emit antipattern site records.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Attribute, Block, Fields, FnArg, ItemEnum, ItemFn, ItemImpl, ItemMod, ItemStruct, ItemTrait,
    ItemType, Signature, TraitItem, Type,
};

use crate::enricher::is_cfg_test;

use super::preds::{
    box_dyn_error_snippet, box_dyn_error_trait_object, cfg_sibling_real_param_names_in_impl_items,
    cfg_sibling_real_param_names_in_items, has_proc_macro_abi_attr, is_creusot_opaque_logic_stub,
    is_stringish_error_type, result_error_type, result_string_error_snippet,
    static_ref_field_snippet, truncate_snippet, type_contains_disallowed_static_ref,
    type_is_location_capture, type_label, unused_argument_bindings,
};
use crate::etiquettes::antipatterns::types::{AntipatternRuleId, AntipatternSiteRecord};

use tracing::instrument;
pub(super) struct AntipatternScanVisitor<'a> {
    pub(super) file: PathBuf,
    pub(super) crate_root: PathBuf,
    pub(super) module_prefix: Vec<String>,
    pub(super) impl_type: Option<String>,
    pub(super) fn_stack: Vec<String>,
    pub(super) in_trait_definition: bool,
    pub(super) in_foreign_trait_impl: bool,
    pub(super) local_trait_names: &'a HashSet<String>,
    pub(super) const_placed_types: &'a HashSet<String>,
    pub(super) cfg_sibling_real_params: HashMap<String, HashSet<String>>,
    pub(super) findings: Vec<AntipatternSiteRecord>,
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

    #[instrument(level = "trace", skip(self))]
    fn rel_file(&self) -> PathBuf {
        self.file
            .strip_prefix(&self.crate_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| self.file.clone())
    }

    #[instrument(level = "debug", skip(self, ty))]
    fn check_box_dyn_error_type(&mut self, ty: &Type) {
        let Some(trait_obj) = box_dyn_error_trait_object(ty) else {
            return;
        };
        self.findings.push(AntipatternSiteRecord {
            rule_id: AntipatternRuleId::BoxDynError001,
            context: self.site_context(),
            file: self.rel_file(),
            line: ty.span().start().line as u32,
            snippet: box_dyn_error_snippet(trait_obj),
        });
    }

    #[instrument(level = "debug", skip(self, ty))]
    fn check_string_error_type(&mut self, ty: &Type) {
        let Some(error_ty) = result_error_type(ty) else {
            return;
        };
        if !is_stringish_error_type(error_ty) {
            return;
        }
        self.findings.push(AntipatternSiteRecord {
            rule_id: AntipatternRuleId::StringError001,
            context: self.site_context(),
            file: self.rel_file(),
            line: ty.span().start().line as u32,
            snippet: truncate_snippet(&result_string_error_snippet(ty), 96),
        });
    }

    #[instrument(level = "debug", skip(self, ty))]
    fn check_adt_field(&mut self, owner: &str, field_name: &str, ty: &Type) {
        if !type_contains_disallowed_static_ref(ty, self.local_trait_names) {
            return;
        }
        let type_name = owner.split("::").next().unwrap_or(owner);
        if !type_is_location_capture(ty) && self.const_placed_types.contains(type_name) {
            return;
        }
        self.findings.push(AntipatternSiteRecord {
            rule_id: AntipatternRuleId::StructStaticRef001,
            context: self.adt_field_context(owner, field_name),
            file: self.rel_file(),
            line: ty.span().start().line as u32,
            snippet: static_ref_field_snippet(ty),
        });
    }

    #[instrument(level = "debug", skip(self, variant))]
    fn check_enum_variant(&mut self, enum_name: &str, variant: &syn::Variant) {
        let owner = format!("{enum_name}::{}", variant.ident);
        match &variant.fields {
            Fields::Named(fields) => self.check_named_fields(&owner, fields),
            Fields::Unnamed(fields) => self.check_unnamed_fields(&owner, fields),
            Fields::Unit => {}
        }
    }

    #[instrument(level = "debug", skip(self))]
    fn adt_field_context(&self, owner: &str, field_name: &str) -> String {
        let mut parts = self.module_prefix.clone();
        parts.push(owner.to_string());
        parts.push(field_name.to_string());
        parts.join("::")
    }

    #[instrument(level = "debug", skip(self, fields))]
    fn check_named_fields(&mut self, owner: &str, fields: &syn::FieldsNamed) {
        for field in &fields.named {
            let name = field
                .ident
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "_".to_string());
            self.check_adt_field(owner, &name, &field.ty);
        }
    }

    #[instrument(level = "debug", skip(self, fields))]
    fn check_unnamed_fields(&mut self, owner: &str, fields: &syn::FieldsUnnamed) {
        for (index, field) in fields.unnamed.iter().enumerate() {
            self.check_adt_field(owner, &format!("_{index}"), &field.ty);
        }
    }

    #[instrument(level = "debug", skip(self, attrs, sig, block))]
    fn check_fn_sig(&mut self, attrs: &[Attribute], sig: &Signature, block: &Block) {
        if self.in_trait_definition || self.in_foreign_trait_impl {
            return;
        }
        if has_proc_macro_abi_attr(attrs) || is_creusot_opaque_logic_stub(attrs, block) {
            return;
        }
        let real_names_elsewhere = self
            .fn_stack
            .last()
            .and_then(|name| self.cfg_sibling_real_params.get(name));
        for arg in &sig.inputs {
            let FnArg::Typed(pat_type) = arg else {
                continue;
            };
            for binding in unused_argument_bindings(&pat_type.pat) {
                if let Some(unprefixed) = binding.snippet.strip_prefix('_')
                    && real_names_elsewhere.is_some_and(|names| names.contains(unprefixed))
                {
                    continue;
                }
                self.findings.push(AntipatternSiteRecord {
                    rule_id: AntipatternRuleId::UnusedUnderscoreArg001,
                    context: self.site_context(),
                    file: self.rel_file(),
                    line: binding.line,
                    snippet: binding.snippet,
                });
            }
        }
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

/// Trait identifiers defined in this crate's scanned sources.
#[instrument(level = "debug", skip(file))]
pub(super) fn collect_local_trait_names(file: &syn::File) -> HashSet<String> {
    let mut collector = TraitNameCollector {
        names: HashSet::new(),
    };
    collector.visit_file(file);
    collector.names
}

struct TraitNameCollector {
    names: HashSet<String>,
}

impl<'ast> Visit<'ast> for TraitNameCollector {
    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        self.names.insert(node.ident.to_string());
        syn::visit::visit_item_trait(self, node);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }
}

#[instrument(level = "trace", skip(node, local_trait_names), ret)]
fn is_foreign_trait_impl(node: &ItemImpl, local_trait_names: &HashSet<String>) -> bool {
    let Some((_, path, _)) = node.trait_.as_ref() else {
        return false;
    };
    let Some(segment) = path.segments.last() else {
        return false;
    };
    !local_trait_names.contains(&segment.ident.to_string())
}
