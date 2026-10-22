//! Check error-shaped types, fields, and function signatures.

use syn::spanned::Spanned;
use syn::{Attribute, Block, Fields, FnArg, Signature, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::etiquettes::antipatterns::types::AntipatternRuleId;

use super::super::preds::{
    box_dyn_error_snippet, box_dyn_error_trait_object, has_proc_macro_abi_attr,
    is_creusot_opaque_logic_stub, is_stringish_error_type, result_error_type,
    result_string_error_snippet, static_ref_field_snippet, truncate_snippet,
    type_contains_disallowed_static_ref, type_is_location_capture, unused_argument_bindings,
};
use super::AntipatternScanVisitor;

impl AntipatternScanVisitor<'_> {
    #[instrument(level = "debug", skip(self, ty))]
    pub(super) fn check_box_dyn_error_type(&mut self, ty: &Type) {
        let Some(trait_obj) = box_dyn_error_trait_object(ty) else {
            return;
        };
        self.push_record(
            AntipatternRuleId::BoxDynError001,
            self.site_context(),
            ty.span().start().line as u32,
            box_dyn_error_snippet(trait_obj),
        );
    }

    #[instrument(level = "debug", skip(self, ty))]
    pub(super) fn check_string_error_type(&mut self, ty: &Type) {
        let Some(error_ty) = result_error_type(ty) else {
            return;
        };
        if !is_stringish_error_type(error_ty) {
            return;
        }
        self.push_record(
            AntipatternRuleId::StringError001,
            self.site_context(),
            ty.span().start().line as u32,
            truncate_snippet(&result_string_error_snippet(ty), 96),
        );
    }

    #[instrument(level = "debug", skip(self, ty))]
    fn check_adt_field(&mut self, owner: &str, field_name: &str, ty: &Type) {
        let type_name = owner.split("::").next().unwrap_or(owner);
        if self.error_field_strategy == ErrorFieldStrategy::Borrowed
            && self.error_type_names.contains(type_name)
            && !type_is_location_capture(ty)
        {
            return;
        }
        if !type_contains_disallowed_static_ref(ty, self.local_trait_names) {
            return;
        }
        if !type_is_location_capture(ty) && self.const_placed_types.contains(type_name) {
            return;
        }
        self.push_record(
            AntipatternRuleId::StructStaticRef001,
            self.adt_field_context(owner, field_name),
            ty.span().start().line as u32,
            static_ref_field_snippet(ty, self.static_ref_strategy),
        );
    }

    #[instrument(level = "debug", skip(self, variant))]
    pub(super) fn check_enum_variant(&mut self, enum_name: &str, variant: &syn::Variant) {
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
    pub(super) fn check_named_fields(&mut self, owner: &str, fields: &syn::FieldsNamed) {
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
    pub(super) fn check_unnamed_fields(&mut self, owner: &str, fields: &syn::FieldsUnnamed) {
        for (index, field) in fields.unnamed.iter().enumerate() {
            self.check_adt_field(owner, &format!("_{index}"), &field.ty);
        }
    }

    #[instrument(level = "debug", skip(self, attrs, sig, block))]
    pub(super) fn check_fn_sig(&mut self, attrs: &[Attribute], sig: &Signature, block: &Block) {
        if self.in_trait_definition || self.in_foreign_trait_impl {
            return;
        }
        if has_proc_macro_abi_attr(attrs) || is_creusot_opaque_logic_stub(attrs, block) {
            return;
        }
        let real_names_elsewhere = self
            .fn_stack
            .last()
            .and_then(|name| self.cfg_sibling_real_params.get(name))
            .cloned();
        for arg in &sig.inputs {
            let FnArg::Typed(pat_type) = arg else {
                continue;
            };
            for binding in unused_argument_bindings(&pat_type.pat) {
                if let Some(unprefixed) = binding.snippet().strip_prefix('_')
                    && real_names_elsewhere
                        .as_ref()
                        .is_some_and(|names| names.contains(unprefixed))
                {
                    continue;
                }
                self.push_record(
                    AntipatternRuleId::UnusedUnderscoreArg001,
                    self.site_context(),
                    binding.line(),
                    binding.snippet().clone(),
                );
            }
        }
    }
}
