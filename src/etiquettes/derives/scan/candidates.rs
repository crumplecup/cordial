use syn::spanned::Spanned;

use super::{DeriveScanVisitor, FieldMeta, SiteArgs, StructInfo, setter_field_name};
use crate::etiquettes::derives::syntax::{
    FieldRead, body_is_struct_literal, classify_field_read, classify_setter_body,
    constructor_arg_count, constructor_fields_match_params, has_derive, has_track_caller,
    receiver_is_immutable_reference, receiver_is_mutable, type_label,
};
use crate::etiquettes::derives::types::DeriveRuleId;

use tracing::instrument;

impl DeriveScanVisitor<'_> {
    #[instrument(level = "debug", skip(self, struct_info, method))]
    pub(super) fn check_getter_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&StructInfo>,
        method: &syn::ImplItemFn,
        method_name: &str,
    ) {
        if method.sig.constness.is_some() {
            return;
        }
        if self.blocked_by_path_inclusion("derive_getters") {
            return;
        }
        let Some(info) = struct_info else {
            return;
        };
        if has_derive(&info.attrs, "Getters") {
            return;
        }
        let Some(field) = info.fields.get(method_name) else {
            return;
        };
        if field.is_public {
            return;
        }
        let Some(recv) = method.sig.receiver() else {
            return;
        };
        if !receiver_is_immutable_reference(recv) {
            return;
        }
        let Some((field_name, read)) = classify_field_read(&method.block) else {
            return;
        };
        if field_name != method_name {
            return;
        }
        if matches!(read, FieldRead::Direct)
            && field
                .boxed_inner_type()
                .as_ref()
                .is_some_and(|inner| method_returns_reference_to(&method.sig, inner))
        {
            return;
        }
        let recommendation = match read {
            FieldRead::Direct => "Use #[derive(derive_getters::Getters)] and delete manual getter",
            // Bare `self.field` (no `.clone()`) only compiles when the
            // field is genuinely Copy -- the type system already proved
            // it, so #[getter(copy)] is a safe recommendation here.
            FieldRead::DirectOwned => {
                "Use #[derive(derive_getters::Getters)] with #[getter(copy)] for Copy fields"
            }
            // `self.field.clone()` proves nothing about Copy. No
            // derive_getters action replicates an owned-clone-returning
            // getter for a non-Copy field.
            FieldRead::Clone | FieldRead::AsStr | FieldRead::AsRef => return,
        };

        self.push_site(SiteArgs::new(
            DeriveRuleId::Getter001,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            recommendation,
            method.span().start().line as u32,
            format!("`fn {method_name}(&self)` returns private field `{method_name}`"),
        ));
    }

    #[instrument(level = "debug", skip(self, struct_info, method))]
    pub(super) fn check_as_ref_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&StructInfo>,
        method: &syn::ImplItemFn,
        method_name: &str,
    ) {
        if method.sig.constness.is_some() {
            return;
        }
        if self.blocked_by_path_inclusion("derive_more") {
            return;
        }
        let Some(info) = struct_info else {
            return;
        };
        if has_derive(&info.attrs, "AsRef") {
            return;
        }
        let Some(recv) = method.sig.receiver() else {
            return;
        };
        if !receiver_is_immutable_reference(recv) {
            return;
        }
        let Some((field_name, read)) = classify_field_read(&method.block) else {
            return;
        };
        // `Option<T>::as_ref()` (`&Option<T> -> Option<&T>`) is a real,
        // distinct std method with a different shape from a field-forwarding
        // `derive_more::AsRef` (`&Self -> &FieldType`).
        if matches!(read, FieldRead::AsRef)
            && info
                .fields
                .get(&field_name)
                .is_some_and(FieldMeta::is_option)
        {
            return;
        }
        let (rule_id, recommendation, evidence) = match read {
            FieldRead::AsRef => (
                DeriveRuleId::AsRef001,
                "Use #[derive(derive_more::AsRef)] and delete the manual as_ref()",
                format!("`fn {method_name}(&self)` forwards `{field_name}.as_ref()`"),
            ),
            FieldRead::AsStr => (
                DeriveRuleId::AsStr001,
                "Use #[derive(derive_more::AsRef)] with #[as_ref] so AsRef<str> replaces as_str()",
                format!("`fn {method_name}(&self)` forwards `{field_name}.as_str()`"),
            ),
            FieldRead::Direct | FieldRead::DirectOwned | FieldRead::Clone => return,
        };

        self.push_site(SiteArgs::new(
            rule_id,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            recommendation,
            method.span().start().line as u32,
            evidence,
        ));
    }

    #[instrument(level = "debug", skip(self, struct_info, method))]
    pub(super) fn check_setter_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&StructInfo>,
        method: &syn::ImplItemFn,
        method_name: &str,
    ) {
        if method.sig.constness.is_some() {
            return;
        }
        if self.blocked_by_path_inclusion("derive_setters") {
            return;
        }
        let Some(info) = struct_info else {
            return;
        };
        if has_derive(&info.attrs, "Setters") {
            return;
        }
        if self_ty.ends_with("Builder") {
            return;
        }
        let Some(field_name) = setter_field_name(method_name) else {
            return;
        };
        if !info.fields.contains_key(field_name) {
            return;
        }
        let Some(recv) = method.sig.receiver() else {
            return;
        };
        if !receiver_is_mutable(recv) {
            return;
        }
        let Some(shape) = classify_setter_body(&method.block, field_name, &method.sig) else {
            return;
        };

        self.push_site(SiteArgs::new(
            DeriveRuleId::Setter001,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            shape.recommendation(),
            method.span().start().line as u32,
            format!("manual setter `{method_name}` on `{self_ty}`"),
        ));
    }

    #[instrument(level = "debug", skip(self, struct_info, method))]
    pub(super) fn check_new_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&StructInfo>,
        method: &syn::ImplItemFn,
        method_name: &str,
    ) {
        if method.sig.constness.is_some() {
            return;
        }
        if self.is_error_constructor(self_ty, method) {
            return;
        }
        if self_ty.ends_with("Builder") {
            return;
        }
        let Some(info) = struct_info else {
            return;
        };
        if has_derive(&info.attrs, "Builder") {
            return;
        }

        if !matches!(method.sig.output, syn::ReturnType::Type(_, _)) {
            return;
        }
        if !body_is_struct_literal(&method.block, self_ty) {
            return;
        }
        if !constructor_fields_match_params(&method.sig, &method.block) {
            return;
        }

        let args = constructor_arg_count(&method.sig);
        if args > self.thresholds.max_constructor_args() {
            if self.blocked_by_path_inclusion("derive_builder") {
                return;
            }
            self.push_site(SiteArgs::new(
                DeriveRuleId::UseBuilder001,
                self_ty,
                Some(method_name.to_string()),
                format!("{self_ty}::{method_name}"),
                format!(
                    "`new` has more than {} arguments; use a builder",
                    self.thresholds.max_constructor_args()
                ),
                method.span().start().line as u32,
                format!(
                    "`fn new` takes {args} arguments (max {})",
                    self.thresholds.max_constructor_args()
                ),
            ));
            return;
        }

        if has_derive(&info.attrs, "new") {
            return;
        }
        if self.blocked_by_path_inclusion("derive_new") {
            return;
        }

        self.push_site(SiteArgs::new(
            DeriveRuleId::New001,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            "Consider #[derive(derive_new::new)] if no validation logic is required",
            method.span().start().line as u32,
            format!(
                "`fn new(...)` fills `{self_ty}` via struct literal (<= {} params)",
                self.thresholds.max_constructor_args()
            ),
        ));
    }

    #[instrument(level = "trace", skip(self, method))]
    fn is_error_constructor(&self, self_ty: &str, method: &syn::ImplItemFn) -> bool {
        self.error_types.contains(self_ty) || has_track_caller(&method.attrs)
    }
}

#[instrument(level = "debug", skip(sig, type_name), ret)]
fn method_returns_reference_to(sig: &syn::Signature, type_name: &str) -> bool {
    let syn::ReturnType::Type(_, ty) = &sig.output else {
        return false;
    };
    let syn::Type::Reference(reference) = ty.as_ref() else {
        return false;
    };
    type_label(&reference.elem) == type_name
}
