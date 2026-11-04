use super::{DeriveScanVisitor, SiteArgs, setter_field_name};
use crate::etiquettes::derives::types::DeriveRuleId;
use crate::ir::{
    DeriveFieldFacts, DeriveFieldRead as FieldRead, DeriveMethodFacts, DeriveStructFacts,
};
use tracing::instrument;

impl DeriveScanVisitor<'_> {
    #[instrument(level = "debug", skip(self, struct_info, method))]
    pub(super) fn check_getter_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&DeriveStructFacts>,
        method: &DeriveMethodFacts,
        method_name: &str,
    ) {
        if method.flags().is_const() {
            return;
        }
        if self.blocked_by_path_inclusion("derive_getters") {
            return;
        }
        let Some(info) = struct_info else {
            return;
        };
        if has_derive(info, "Getters") {
            return;
        }
        let Some(field) = info.fields().get(method_name) else {
            return;
        };
        if field.is_public() {
            return;
        }
        if !method.flags().immutable_reference() {
            return;
        }
        let Some((field_name, read)) = method.field_read().as_ref() else {
            return;
        };
        if field_name != method_name {
            return;
        }
        if matches!(read, FieldRead::Direct)
            && field
                .boxed_inner_type()
                .as_ref()
                .is_some_and(|inner| method.return_reference().as_ref() == Some(inner))
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
            method.line(),
            format!("`fn {method_name}(&self)` returns private field `{method_name}`"),
        ));
    }

    #[instrument(level = "debug", skip(self, struct_info, method))]
    pub(super) fn check_as_ref_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&DeriveStructFacts>,
        method: &DeriveMethodFacts,
        method_name: &str,
    ) {
        if method.flags().is_const() {
            return;
        }
        if self.blocked_by_path_inclusion("derive_more") {
            return;
        }
        let Some(info) = struct_info else {
            return;
        };
        if has_derive(info, "AsRef") {
            return;
        }
        if !method.flags().immutable_reference() {
            return;
        }
        let Some((field_name, read)) = method.field_read().as_ref() else {
            return;
        };
        // `Option<T>::as_ref()` (`&Option<T> -> Option<&T>`) is a real,
        // distinct std method with a different shape from a field-forwarding
        // `derive_more::AsRef` (`&Self -> &FieldType`).
        if matches!(read, FieldRead::AsRef)
            && info
                .fields()
                .get(field_name)
                .is_some_and(DeriveFieldFacts::is_option)
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
            method.line(),
            evidence,
        ));
    }

    #[instrument(level = "debug", skip(self, struct_info, method))]
    pub(super) fn check_setter_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&DeriveStructFacts>,
        method: &DeriveMethodFacts,
        method_name: &str,
    ) {
        if method.flags().is_const() {
            return;
        }
        if self.blocked_by_path_inclusion("derive_setters") {
            return;
        }
        let Some(info) = struct_info else {
            return;
        };
        if has_derive(info, "Setters") {
            return;
        }
        if self_ty.ends_with("Builder") {
            return;
        }
        let Some(field_name) = setter_field_name(method_name) else {
            return;
        };
        if !info.fields().contains_key(field_name) {
            return;
        }
        if !method.flags().mutable_receiver() {
            return;
        }
        let Some(shape) = method.setter() else {
            return;
        };

        self.push_site(SiteArgs::new(
            DeriveRuleId::Setter001,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            shape.recommendation(),
            method.line(),
            format!("manual setter `{method_name}` on `{self_ty}`"),
        ));
    }

    #[instrument(level = "debug", skip(self, struct_info, method))]
    pub(super) fn check_new_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&DeriveStructFacts>,
        method: &DeriveMethodFacts,
        method_name: &str,
    ) {
        if method.flags().is_const() {
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
        if has_derive(info, "Builder") {
            return;
        }

        let Some(args) = method.constructor() else {
            return;
        };
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
                method.line(),
                format!(
                    "`fn new` takes {args} arguments (max {})",
                    self.thresholds.max_constructor_args()
                ),
            ));
            return;
        }

        if has_derive(info, "new") {
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
            method.line(),
            format!(
                "`fn new(...)` fills `{self_ty}` via struct literal (<= {} params)",
                self.thresholds.max_constructor_args()
            ),
        ));
    }

    #[instrument(level = "trace", skip(self, method))]
    fn is_error_constructor(&self, self_ty: &str, method: &DeriveMethodFacts) -> bool {
        self.error_types.contains(self_ty) || method.flags().track_caller()
    }
}

#[instrument(level = "trace", skip(info))]
pub(super) fn has_derive(info: &DeriveStructFacts, needle: &str) -> bool {
    info.derives().iter().any(|name| name == needle)
}
