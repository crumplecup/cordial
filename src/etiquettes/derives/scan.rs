//! syn-based scan for manual builders, getters, setters, and `new()`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::{Fields, FieldsNamed, Item, ItemImpl, ItemMod, ItemStruct};

use crate::config::DerivesThresholds;
use crate::error::CordialResult;
use crate::loader::module_path_from_src_file;

use super::syntax::{
    FieldRead, body_is_struct_literal, classify_field_read, classify_setter_body,
    constructor_arg_count, consumes_self, error_impl_target, field_is_exposed, has_derive,
    has_track_caller, is_cfg_test, is_clap_schema, is_fluent_setter, type_label,
};
use super::types::{DeriveRuleId, DeriveSiteRecord};

use tracing::instrument;

#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_source_tree(
    src_root: &Path,
    crate_root: &Path,
    thresholds: DerivesThresholds,
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
        findings.extend(scan_rust_source(
            &source, path, src_root, crate_root, thresholds,
        )?);
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
    thresholds: DerivesThresholds,
) -> CordialResult<Vec<DeriveSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = DeriveScanVisitor {
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        structs: HashMap::new(),
        error_types: HashSet::new(),
        thresholds,
        findings: Vec::new(),
    };
    visitor.walk_items(&syntax.items);
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
    error_types: HashSet<String>,
    thresholds: DerivesThresholds,
    findings: Vec<DeriveSiteRecord>,
}

impl DeriveScanVisitor {
    #[instrument(level = "debug", skip(self))]
    fn qualify(&self, local: &str) -> String {
        if self.module_prefix.is_empty() {
            local.to_string()
        } else {
            format!("{}::{local}", self.module_prefix.join("::"))
        }
    }

    #[instrument(
        level = "trace",
        skip(self, rule_id, struct_name, recommendation, evidence)
    )]
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

    #[instrument(level = "debug", skip(self, record))]
    fn push_finding(&mut self, mut record: DeriveSiteRecord) {
        if let Ok(rel) = record.file.strip_prefix(&self.crate_root) {
            record.file = rel.to_path_buf();
        }
        self.findings.push(record);
    }

    #[instrument(level = "debug", skip(self, items))]
    fn walk_items(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Struct(item_struct) => self.register_struct(item_struct),
                Item::Impl(item_impl) => {
                    if let Some(name) = error_impl_target(item_impl) {
                        self.error_types.insert(name);
                    }
                }
                Item::Mod(item_mod) => self.walk_mod(item_mod),
                _ => {}
            }
        }
        for item in items {
            if let Item::Impl(item_impl) = item
                && item_impl.trait_.is_none()
            {
                self.visit_impl(item_impl);
            }
        }
    }

    #[instrument(level = "debug", skip(self, item_mod))]
    fn walk_mod(&mut self, item_mod: &ItemMod) {
        if is_cfg_test(&item_mod.attrs) {
            return;
        }
        let Some((_, items)) = &item_mod.content else {
            return;
        };
        let mut nested = self.module_prefix.clone();
        nested.push(item_mod.ident.to_string());
        let previous = std::mem::replace(&mut self.module_prefix, nested);
        self.walk_items(items);
        self.module_prefix = previous;
    }

    #[instrument(level = "debug", skip(self, item_struct))]
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
        if exposed_fields.is_empty() || is_clap_schema(&item_struct.attrs) {
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

    #[instrument(level = "debug", skip(self, item_impl))]
    fn visit_impl(&mut self, item_impl: &ItemImpl) {
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

    #[instrument(level = "debug", skip(self, struct_info, method))]
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
        if is_fluent_setter(&method.sig)
            && classify_setter_body(
                &method.block,
                setter_target_field(&method_name),
                &method.sig,
            )
            .is_some()
        {
            fluent_setters.push((method_name.clone(), line));
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
        if !fluent_setters.is_empty() && fluent_setters.len() >= self.thresholds.min_fluent_setters()
        {
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

    #[instrument(level = "debug", skip(self, struct_info, method))]
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
        let Some((field_name, read)) = classify_field_read(&method.block) else {
            return;
        };
        if field_name != method_name {
            return;
        }
        let recommendation = match read {
            FieldRead::Direct => "Use #[derive(derive_getters::Getters)] and delete manual getter",
            FieldRead::Clone => {
                "Use #[derive(derive_getters::Getters)] with #[getter(copy)] for Copy fields"
            }
            FieldRead::AsStr | FieldRead::AsRef => return,
        };

        let record = self.site(
            DeriveRuleId::Getter001,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            recommendation,
            method.span().start().line as u32,
            format!("`fn {method_name}(&self)` returns private field `{method_name}`"),
        );
        self.push_finding(record);
    }

    #[instrument(level = "debug", skip(self, struct_info, method))]
    fn check_as_ref_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&StructInfo>,
        method: &syn::ImplItemFn,
        method_name: &str,
    ) {
        let Some(info) = struct_info else {
            return;
        };
        if has_derive(&info.attrs, "AsRef") {
            return;
        }
        let Some(recv) = method.sig.receiver() else {
            return;
        };
        if recv.mutability.is_some() || recv.reference.is_none() {
            return;
        }
        let Some((field_name, read)) = classify_field_read(&method.block) else {
            return;
        };
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
            FieldRead::Direct | FieldRead::Clone => return,
        };

        let record = self.site(
            rule_id,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            recommendation,
            method.span().start().line as u32,
            evidence,
        );
        self.push_finding(record);
    }

    #[instrument(level = "debug", skip(self, struct_info, method))]
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
        let Some(shape) = classify_setter_body(&method.block, field_name, &method.sig) else {
            return;
        };

        let record = self.site(
            DeriveRuleId::Setter001,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            shape.recommendation(),
            method.span().start().line as u32,
            format!("manual setter `{method_name}` on `{self_ty}`"),
        );
        self.push_finding(record);
    }

    #[instrument(level = "debug", skip(self, struct_info, method))]
    fn check_new_candidate(
        &mut self,
        self_ty: &str,
        struct_info: Option<&StructInfo>,
        method: &syn::ImplItemFn,
        method_name: &str,
    ) {
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

        let args = constructor_arg_count(&method.sig);
        if args > self.thresholds.max_constructor_args() {
            let record = self.site(
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
            );
            self.push_finding(record);
            return;
        }

        if has_derive(&info.attrs, "new") {
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
            format!(
                "`fn new(…)` fills `{self_ty}` via struct literal (≤{} params)",
                self.thresholds.max_constructor_args()
            ),
        );
        self.push_finding(record);
    }

    #[instrument(level = "debug", skip(self, method))]
    fn is_error_constructor(&self, self_ty: &str, method: &syn::ImplItemFn) -> bool {
        self.error_types.contains(self_ty) || has_track_caller(&method.attrs)
    }
}

#[instrument(level = "debug", skip(item_struct))]
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

#[instrument(level = "debug")]
fn setter_field_name(method_name: &str) -> Option<&str> {
    method_name
        .strip_prefix("with_")
        .or_else(|| method_name.strip_prefix("set_"))
}

#[instrument(level = "debug")]
fn setter_target_field(method_name: &str) -> &str {
    setter_field_name(method_name).unwrap_or(method_name)
}
