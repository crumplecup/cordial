//! syn-based scan for manual builders, getters, setters, and `new()`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Fields, FieldsNamed, Item, ItemImpl, ItemMod, ItemStruct};

use crate::error::CordialResult;
use crate::loader::module_path_from_src_file;

use super::syntax::{
    body_is_struct_literal, body_is_trivial_field_access, consumes_self, field_is_exposed,
    has_derive, is_cfg_test, is_fluent_setter, type_label,
};
use super::types::{DeriveRuleId, DeriveSiteRecord};

use tracing::instrument;
const MAX_NEW_PARAMS: usize = 4;

#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_source_tree(
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<DeriveSiteRecord>> {
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
        let source = std::fs::read_to_string(path)?;
        findings.extend(scan_rust_source(&source, path, src_root, crate_root)?);
    }

    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.qualified_name.cmp(&b.qualified_name))
    });

    Ok(findings)
}

#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<DeriveSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = DeriveScanVisitor {
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        structs: HashMap::new(),
        findings: Vec::new(),
    };
    visitor.visit_file(&syntax);
    Ok(visitor.findings)
}

#[derive(Debug, Clone)]
struct StructInfo {
    attrs: Vec<syn::Attribute>,
    fields: HashMap<String, FieldMeta>,
}

#[derive(Debug, Clone)]
struct FieldMeta {
    is_public: bool,
}

struct DeriveScanVisitor {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    structs: HashMap<String, StructInfo>,
    findings: Vec<DeriveSiteRecord>,
}

impl DeriveScanVisitor {
    fn qualify(&self, local: &str) -> String {
        if self.module_prefix.is_empty() {
            local.to_string()
        } else {
            format!("{}::{local}", self.module_prefix.join("::"))
        }
    }

    fn site(
        &self,
        rule_id: DeriveRuleId,
        struct_name: impl Into<String>,
        method_name: Option<String>,
        qualified_local: String,
        recommendation: impl Into<String>,
        line: u32,
        evidence: impl Into<String>,
    ) -> DeriveSiteRecord {
        DeriveSiteRecord {
            rule_id,
            struct_name: struct_name.into(),
            method_name,
            qualified_name: self.qualify(&qualified_local),
            recommendation: recommendation.into(),
            file: self.file.clone(),
            line,
            evidence: evidence.into(),
        }
    }

    fn push_finding(&mut self, mut record: DeriveSiteRecord) {
        if let Ok(rel) = record.file.strip_prefix(&self.crate_root) {
            record.file = rel.to_path_buf();
        }
        self.findings.push(record);
    }

    fn visit_module_items(&mut self, items: &[Item], module_prefix: &[String]) {
        let prev_prefix = self.module_prefix.clone();
        self.module_prefix = module_prefix.to_vec();
        for item in items {
            self.visit_item(item);
        }
        self.module_prefix = prev_prefix;
    }

    fn visit_item(&mut self, item: &Item) {
        match item {
            Item::Struct(item_struct) => self.register_struct(item_struct),
            Item::Mod(item_mod) => self.visit_mod(item_mod),
            Item::Impl(item_impl) => self.visit_impl(item_impl),
            _ => {}
        }
    }

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

    fn register_struct(&mut self, item_struct: &ItemStruct) {
        let name = item_struct.ident.to_string();
        let (fields, exposed_fields) = collect_struct_fields(item_struct);
        self.structs.insert(
            name.clone(),
            StructInfo {
                attrs: item_struct.attrs.clone(),
                fields,
            },
        );
        if exposed_fields.is_empty() {
            return;
        }
        let field_list = exposed_fields
            .iter()
            .map(|field| format!("`{field}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let record = self.site(
            DeriveRuleId::PubField001,
            name.clone(),
            None,
            name,
            "Make fields private; use derive_getters, derive_setters, \
             derive_new, or derive_builder instead of struct literals",
            item_struct.span().start().line as u32,
            format!("non-private fields: {field_list}"),
        );
        self.push_finding(record);
    }

    fn visit_impl(&mut self, item_impl: &ItemImpl) {
        if item_impl.trait_.is_some() {
            return;
        }
        let self_ty = type_label(&item_impl.self_ty);
        let struct_info = self.structs.get(&self_ty).cloned();
        if struct_info
            .as_ref()
            .is_some_and(|info| has_derive(&info.attrs, "Builder"))
        {
            return;
        }

        let mut fluent_setters = Vec::new();
        let mut build_line = None;
        for impl_item in &item_impl.items {
            let syn::ImplItem::Fn(method) = impl_item else {
                continue;
            };
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

    fn inspect_impl_method(
        &mut self,
        self_ty: &str,
        struct_info: Option<&StructInfo>,
        method: &syn::ImplItemFn,
        fluent_setters: &mut Vec<(String, u32)>,
        build_line: &mut Option<u32>,
    ) {
        let method_name = method.sig.ident.to_string();
        let line = method.span().start().line as u32;
        if method_name == "build" && consumes_self(&method.sig) {
            *build_line = Some(line);
        }
        if is_fluent_setter(&method.sig) {
            fluent_setters.push((method_name.clone(), line));
        }
        if method_name == "new" {
            self.check_new_candidate(self_ty, struct_info, method, &method_name);
        }
        if method_name.starts_with("with_") || method_name.starts_with("set_") {
            self.check_setter_candidate(self_ty, struct_info, method, &method_name);
        }
        self.check_getter_candidate(self_ty, struct_info, method, &method_name);
    }

    fn maybe_flag_manual_builder(
        &mut self,
        self_ty: &str,
        item_impl: &ItemImpl,
        fluent_setters: &[(String, u32)],
        build_line: Option<u32>,
    ) {
        let recommendation = "Use #[derive(derive_builder::Builder)] on the built type";
        if self_ty.ends_with("Builder") {
            let record = self.site(
                DeriveRuleId::Builder001,
                self_ty,
                None,
                self_ty.to_string(),
                recommendation,
                item_impl.span().start().line as u32,
                format!("type `{self_ty}` ends with `Builder`"),
            );
            self.push_finding(record);
            return;
        }
        if let Some(line) = build_line {
            let record = self.site(
                DeriveRuleId::Builder001,
                self_ty,
                Some("build".to_string()),
                format!("{self_ty}::build"),
                recommendation,
                line,
                format!("`{self_ty}::build(self) -> …`"),
            );
            self.push_finding(record);
            return;
        }
        if fluent_setters.len() >= 2 {
            let (name, line) = &fluent_setters[0];
            let record = self.site(
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
            );
            self.push_finding(record);
        }
    }

    fn check_getter_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&StructInfo>,
        method: &syn::ImplItemFn,
        method_name: &str,
    ) {
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
        if recv.mutability.is_some() || recv.reference.is_none() {
            return;
        }
        if !body_is_trivial_field_access(&method.block, method_name) {
            return;
        }

        let record = self.site(
            DeriveRuleId::Getter001,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            "Use #[derive(derive_getters::Getters)] and delete manual getter",
            method.span().start().line as u32,
            format!("`fn {method_name}(&self)` returns private field `{method_name}`"),
        );
        self.push_finding(record);
    }

    fn check_setter_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&StructInfo>,
        method: &syn::ImplItemFn,
        method_name: &str,
    ) {
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
        if recv.mutability.is_none() {
            return;
        }

        let record = self.site(
            DeriveRuleId::Setter001,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            "Use #[derive(derive_setters::Setters)] with #[setters(prefix = \"with_\")]",
            method.span().start().line as u32,
            format!("manual setter `{method_name}` on `{self_ty}`"),
        );
        self.push_finding(record);
    }

    fn check_new_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&StructInfo>,
        method: &syn::ImplItemFn,
        method_name: &str,
    ) {
        let Some(info) = struct_info else {
            return;
        };
        if has_derive(&info.attrs, "new") {
            return;
        }
        if method.sig.inputs.len() > MAX_NEW_PARAMS + 1 {
            return;
        }
        if !matches!(method.sig.output, syn::ReturnType::Type(_, _)) {
            return;
        }
        if !body_is_struct_literal(&method.block, self_ty) {
            return;
        }

        let record = self.site(
            DeriveRuleId::New001,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            "Consider #[derive(derive_new::new)] if no validation logic is required",
            method.span().start().line as u32,
            format!("`fn new(…)` fills `{self_ty}` via struct literal (≤{MAX_NEW_PARAMS} params)"),
        );
        self.push_finding(record);
    }
}

fn collect_struct_fields(item_struct: &ItemStruct) -> (HashMap<String, FieldMeta>, Vec<String>) {
    let mut fields = HashMap::new();
    let mut exposed_fields = Vec::new();
    match &item_struct.fields {
        Fields::Named(FieldsNamed { named, .. }) => {
            for field in named {
                let Some(ident) = &field.ident else {
                    continue;
                };
                let field_name = ident.to_string();
                let exposed = field_is_exposed(&field.vis);
                fields.insert(field_name.clone(), FieldMeta { is_public: exposed });
                if exposed {
                    exposed_fields.push(field_name);
                }
            }
        }
        Fields::Unnamed(fields_unnamed) => {
            for (index, field) in fields_unnamed.unnamed.iter().enumerate() {
                if field_is_exposed(&field.vis) {
                    exposed_fields.push(format!("_{index}"));
                }
            }
        }
        Fields::Unit => {}
    }
    (fields, exposed_fields)
}

fn setter_field_name(method_name: &str) -> Option<&str> {
    method_name
        .strip_prefix("with_")
        .or_else(|| method_name.strip_prefix("set_"))
}

impl<'ast> Visit<'ast> for DeriveScanVisitor {
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        self.register_struct(node);
    }

    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.visit_impl(node);
    }
}
