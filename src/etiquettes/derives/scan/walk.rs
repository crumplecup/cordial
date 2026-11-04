use super::candidates::has_derive;
use super::{DeriveScanVisitor, SiteArgs};
use crate::etiquettes::derives::types::DeriveRuleId;
use crate::ir::{
    DeriveImplFacts, DeriveItemFacts, DeriveMethodFacts, DeriveModuleFacts, DeriveStructFacts,
};
use tracing::instrument;

impl DeriveScanVisitor<'_> {
    #[instrument(level = "debug", skip(self, items))]
    pub(super) fn walk_items(&mut self, items: &[DeriveItemFacts]) {
        for item in items {
            match item {
                DeriveItemFacts::Struct(item_struct) => self.register_struct(item_struct),
                DeriveItemFacts::Impl(item_impl) => {
                    if item_impl.error_impl() {
                        self.error_types.insert(item_impl.self_type().clone());
                    }
                }
                DeriveItemFacts::Module(item_mod) => self.walk_mod(item_mod),
            }
        }
        for item in items {
            if let DeriveItemFacts::Impl(item_impl) = item
                && !item_impl.trait_impl()
            {
                self.visit_impl(item_impl);
            }
        }
    }

    #[instrument(level = "debug", skip(self, item_mod))]
    fn walk_mod(&mut self, item_mod: &DeriveModuleFacts) {
        if item_mod.test_only() {
            return;
        }
        let items = item_mod.items();
        let mut nested = self.module_prefix.clone();
        nested.push(item_mod.name().clone());
        let previous_prefix = std::mem::replace(&mut self.module_prefix, nested);
        // Sticky, not reset per-mod: a plain nested `mod` inside a
        // `#[cfg(creusot)]` ancestor is still only compiled under that
        // same gate, so a struct two levels down still counts as
        // cfg(creusot)-only even though this particular `mod` carries
        // no attribute of its own.
        let previous_cfg_creusot = self.in_cfg_creusot_mod;
        if item_mod.creusot_only() {
            self.in_cfg_creusot_mod = true;
        }
        self.walk_items(items);
        self.module_prefix = previous_prefix;
        self.in_cfg_creusot_mod = previous_cfg_creusot;
    }

    #[instrument(level = "debug", skip(self, item_struct))]
    fn register_struct(&mut self, item_struct: &DeriveStructFacts) {
        let name = item_struct.name().clone();
        let exposed_fields = item_struct.exposed_fields();
        self.structs.insert(name.clone(), item_struct.clone());
        self.generated_builders
            .extend(item_struct.generated_builders().iter().cloned());
        if exposed_fields.is_empty()
            || item_struct.clap_schema()
            || self.in_cfg_creusot_mod
            || item_struct.creusot_only()
        {
            return;
        }
        let field_list = exposed_fields
            .iter()
            .map(|field| format!("`{field}`"))
            .collect::<Vec<_>>()
            .join(", ");
        self.push_site(SiteArgs::new(
            DeriveRuleId::PubField001,
            name.clone(),
            None,
            name,
            "Make fields private; use derive_getters, derive_setters, \
             derive_new, or derive_builder instead of struct literals",
            item_struct.line(),
            format!("non-private fields: {field_list}"),
        ));
    }

    #[instrument(level = "debug", skip(self, item_impl))]
    fn visit_impl(&mut self, item_impl: &DeriveImplFacts) {
        let self_ty = item_impl.self_type().clone();
        if self.generated_builders.contains(&self_ty) {
            return;
        }
        let struct_info = self.structs.get(&self_ty).cloned();
        if struct_info
            .as_ref()
            .is_some_and(|info| has_derive(info, "Builder"))
        {
            return;
        }

        let mut fluent_setters = Vec::new();
        let mut build_line = None;
        for method in item_impl.methods() {
            self.inspect_impl_method(
                &self_ty,
                struct_info.as_ref(),
                method,
                &mut fluent_setters,
                &mut build_line,
            );
        }
        self.maybe_flag_manual_builder(&self_ty, item_impl, &fluent_setters, build_line);
    }

    #[instrument(level = "debug", skip(self, struct_info, method))]
    fn inspect_impl_method(
        &mut self,
        self_ty: &str,
        struct_info: Option<&DeriveStructFacts>,
        method: &DeriveMethodFacts,
        fluent_setters: &mut Vec<(String, u32, bool)>,
        build_line: &mut Option<(u32, bool)>,
    ) {
        let method_name = method.name().clone();
        let line = method.line();
        let is_const = method.flags().is_const();
        if method_name == "build" && method.flags().consumes_self() {
            *build_line = Some((line, is_const));
        }
        if method.flags().fluent_setter() && method.setter().is_some() {
            fluent_setters.push((method_name.clone(), line, is_const));
        }
        if method_name == "new" {
            self.check_new_candidate(self_ty, struct_info, method, &method_name);
        }
        if method_name.starts_with("with_") || method_name.starts_with("set_") {
            self.check_setter_candidate(self_ty, struct_info, method, &method_name);
        }
        self.check_getter_candidate(self_ty, struct_info, method, &method_name);
        self.check_as_ref_candidate(self_ty, struct_info, method, &method_name);
    }

    #[instrument(level = "debug", skip(self, item_impl))]
    fn maybe_flag_manual_builder(
        &mut self,
        self_ty: &str,
        item_impl: &DeriveImplFacts,
        fluent_setters: &[(String, u32, bool)],
        build_line: Option<(u32, bool)>,
    ) {
        let recommendation = "Use #[derive(derive_builder::Builder)] on the built type";
        if self_ty.ends_with("Builder") {
            self.push_site(SiteArgs::new(
                DeriveRuleId::Builder001,
                self_ty,
                None,
                self_ty.to_string(),
                recommendation,
                item_impl.line(),
                format!("type `{self_ty}` ends with `Builder`"),
            ));
            return;
        }
        if let Some((line, is_const)) = build_line {
            // `derive_builder` generates ordinary (non-`const`) methods --
            // a hand-written `const fn build` is real evidence the type
            // needs to stay `const`-constructible (e.g. for a `&'static
            // [T]` array literal via rvalue static promotion, or a value
            // passed to `inventory::submit!`, which requires a
            // `const`-evaluable expression), not an oversight `derive_
            // builder` would just as well replace.
            if !is_const {
                self.push_site(SiteArgs::new(
                    DeriveRuleId::Builder001,
                    self_ty,
                    Some("build".to_string()),
                    format!("{self_ty}::build"),
                    recommendation,
                    line,
                    format!("`{self_ty}::build(self) -> …`"),
                ));
            }
            return;
        }
        if !fluent_setters.is_empty()
            && fluent_setters.len() >= self.thresholds.min_fluent_setters()
        {
            // Same `const fn` exemption as `build`, above -- only when
            // *every* fluent setter found is `const` is the whole chain
            // (constructor included, by construction: a non-`const`
            // constructor feeding a `const fn` setter couldn't be called
            // from a `const` context anyway) genuinely incompatible with
            // `derive_builder`.
            if fluent_setters.iter().all(|(_, _, is_const)| *is_const) {
                return;
            }
            let (name, line, _) = &fluent_setters[0];
            self.push_site(SiteArgs::new(
                DeriveRuleId::Builder001,
                self_ty,
                Some(name.clone()),
                format!("{self_ty}::{name}"),
                recommendation,
                *line,
                format!(
                    "`{self_ty}` has {} fluent setter methods (e.g. `{name}`)",
                    fluent_setters.len()
                ),
            ));
        }
    }
}
