//! syn-based scan for manual builders, getters, setters, and `new()`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::{Fields, FieldsNamed, Item, ItemImpl, ItemMod, ItemStruct};

use crate::config::DerivesThresholds;
use crate::error::CordialResult;
use crate::loader::module_path_from_src_file;

use super::path_inclusion::PathInclusionFacts;
use super::syntax::{
    FieldRead, body_is_struct_literal, classify_field_read, classify_setter_body,
    constructor_arg_count, constructor_fields_match_params, consumes_self, error_impl_target,
    field_is_exposed, has_derive, has_track_caller, is_cfg_test, is_clap_schema, is_fluent_setter,
    type_label,
};
use super::types::{DeriveRuleId, DeriveSiteRecord};

use tracing::instrument;

#[instrument(level = "debug", skip(path_inclusions), err(level = "warn"))]
pub fn scan_source_tree(
    src_root: &Path,
    crate_root: &Path,
    thresholds: DerivesThresholds,
    path_inclusions: &PathInclusionFacts,
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
            &source,
            path,
            src_root,
            crate_root,
            thresholds,
            path_inclusions,
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

#[instrument(
    level = "debug",
    skip(source, file, path_inclusions),
    err(level = "warn")
)]
/// Scan one Rust source file and return records.
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
    thresholds: DerivesThresholds,
    path_inclusions: &PathInclusionFacts,
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
        path_inclusions,
        findings: Vec::new(),
    };
    visitor.walk_items(&syntax.items);
    Ok(visitor.findings)
}

/// Every fact needed to build one [`DeriveSiteRecord`], bundled so
/// [`DeriveScanVisitor::site`] takes one argument instead of seven.
#[derive(derive_new::new)]
struct SiteArgs {
    rule_id: DeriveRuleId,
    #[new(into)]
    struct_name: String,
    method_name: Option<String>,
    #[new(into)]
    qualified_local: String,
    #[new(into)]
    recommendation: String,
    line: u32,
    #[new(into)]
    evidence: String,
}

#[derive(Debug, Clone)]
struct StructInfo {
    attrs: Vec<syn::Attribute>,
    fields: HashMap<String, FieldMeta>,
}

#[derive(Debug, Clone)]
struct FieldMeta {
    is_public: bool,
    is_option: bool,
}

impl FieldMeta {
    #[instrument(level = "trace", skip(self), ret)]
    fn is_option(&self) -> bool {
        self.is_option
    }
}

struct DeriveScanVisitor<'a> {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    structs: HashMap<String, StructInfo>,
    error_types: HashSet<String>,
    thresholds: DerivesThresholds,
    path_inclusions: &'a PathInclusionFacts,
    findings: Vec<DeriveSiteRecord>,
}

impl DeriveScanVisitor<'_> {
    /// True when some *other* crate splices this file in via `#[path]`
    /// without `needed_dep` -- the recommended derive wouldn't actually
    /// compile everywhere this file's content lands.
    #[instrument(level = "trace", skip(self))]
    fn blocked_by_path_inclusion(&self, needed_dep: &str) -> bool {
        if let Some(blocker) =
            self.path_inclusions
                .blocking_consumer(&self.file, &self.crate_root, needed_dep)
        {
            tracing::debug!(
                blocker,
                needed_dep,
                file = %self.file.display(),
                "derive recommendation blocked: consuming crate lacks the dependency"
            );
            return true;
        }
        false
    }

    #[instrument(level = "debug", skip(self))]
    fn qualify(&self, local: &str) -> String {
        if self.module_prefix.is_empty() {
            local.to_string()
        } else {
            format!("{}::{local}", self.module_prefix.join("::"))
        }
    }

    #[instrument(level = "trace", skip(self, args))]
    fn site(&self, args: SiteArgs) -> DeriveSiteRecord {
        DeriveSiteRecord {
            rule_id: args.rule_id,
            struct_name: args.struct_name,
            method_name: args.method_name,
            qualified_name: self.qualify(&args.qualified_local),
            recommendation: args.recommendation,
            file: self.file.clone(),
            line: args.line,
            evidence: args.evidence,
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
        let record = self.site(SiteArgs::new(
            DeriveRuleId::PubField001,
            name.clone(),
            None,
            name,
            "Make fields private; use derive_getters, derive_setters, \
             derive_new, or derive_builder instead of struct literals",
            item_struct.span().start().line as u32,
            format!("non-private fields: {field_list}"),
        ));
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
            let record = self.site(SiteArgs::new(
                DeriveRuleId::Builder001,
                self_ty,
                None,
                self_ty.to_string(),
                recommendation,
                item_impl.span().start().line as u32,
                format!("type `{self_ty}` ends with `Builder`"),
            ));
            self.push_finding(record);
            return;
        }
        if let Some(line) = build_line {
            let record = self.site(SiteArgs::new(
                DeriveRuleId::Builder001,
                self_ty,
                Some("build".to_string()),
                format!("{self_ty}::build"),
                recommendation,
                line,
                format!("`{self_ty}::build(self) -> …`"),
            ));
            self.push_finding(record);
            return;
        }
        if !fluent_setters.is_empty()
            && fluent_setters.len() >= self.thresholds.min_fluent_setters()
        {
            let (name, line) = &fluent_setters[0];
            let record = self.site(SiteArgs::new(
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
            // Bare `self.field` (no `.clone()`) only compiles when the
            // field is genuinely Copy -- the type system already proved
            // it, so #[getter(copy)] (derive_getters' only owned-return
            // action; confirmed against its own source, no "clone"
            // variant exists) is a safe recommendation here.
            FieldRead::DirectOwned => {
                "Use #[derive(derive_getters::Getters)] with #[getter(copy)] for Copy fields"
            }
            // `self.field.clone()` proves nothing about Copy -- and in a
            // codebase clean under clippy::clone_on_copy (warn-by-
            // default), an explicit `.clone()` essentially never appears
            // on a genuinely Copy field, so #[getter(copy)] would almost
            // always fail to compile here. No other derive_getters
            // action replicates an owned-clone-returning getter for a
            // non-Copy field (confirmed against its source: only
            // skip/rename/copy exist), so there's no derive to recommend.
            FieldRead::Clone | FieldRead::AsStr | FieldRead::AsRef => return,
        };

        let record = self.site(SiteArgs::new(
            DeriveRuleId::Getter001,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            recommendation,
            method.span().start().line as u32,
            format!("`fn {method_name}(&self)` returns private field `{method_name}`"),
        ));
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
        if recv.mutability.is_some() || recv.reference.is_none() {
            return;
        }
        let Some((field_name, read)) = classify_field_read(&method.block) else {
            return;
        };
        // `Option<T>::as_ref()` (`&Option<T> -> Option<&T>`) is a real,
        // distinct std method with a completely different shape from a
        // field-forwarding `derive_more::AsRef` (`&Self -> &FieldType`).
        // Method name doesn't distinguish them (both idioms exist under
        // any name), but the field's own declared type does: only a
        // literal `AsRef`-trait forward can ever be replaced by the
        // derive, and `Option<T>` fields never carry that meaning here.
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

        let record = self.site(SiteArgs::new(
            rule_id,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            recommendation,
            method.span().start().line as u32,
            evidence,
        ));
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
        if recv.mutability.is_none() {
            return;
        }
        let Some(shape) = classify_setter_body(&method.block, field_name, &method.sig) else {
            return;
        };

        let record = self.site(SiteArgs::new(
            DeriveRuleId::Setter001,
            self_ty,
            Some(method_name.to_string()),
            format!("{self_ty}::{method_name}"),
            shape.recommendation(),
            method.span().start().line as u32,
            format!("manual setter `{method_name}` on `{self_ty}`"),
        ));
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
        // Both DERIVE-NEW-001 and DERIVE-USE-BUILDER-001 recommend a
        // derive that fills the struct's own fields directly from the
        // constructor's arguments -- a body with real logic (computed
        // fields, a conditional, more than a `let` or two before the
        // literal) can't be replicated by either, regardless of arg
        // count. Real case that would otherwise slip through the arity-
        // only check: `KaniRecursiveDirObservation::new`'s 4 arguments
        // don't map onto its 3 fields at all -- it joins path segments
        // to *compute* them.
        if !body_is_struct_literal(&method.block, self_ty) {
            return;
        }
        // A tail expression can syntactically be `Self { .. }` while still
        // computing a field from unrelated parameters, wrapping a param in
        // another call, or hardcoding a field no parameter names at all --
        // none of which `derive_new`'s param-per-field model can replicate.
        if !constructor_fields_match_params(&method.sig, &method.block) {
            return;
        }

        let args = constructor_arg_count(&method.sig);
        if args > self.thresholds.max_constructor_args() {
            if self.blocked_by_path_inclusion("derive_builder") {
                return;
            }
            let record = self.site(SiteArgs::new(
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
            self.push_finding(record);
            return;
        }

        if has_derive(&info.attrs, "new") {
            return;
        }
        if self.blocked_by_path_inclusion("derive_new") {
            return;
        }

        let record = self.site(SiteArgs::new(
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
        ));
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
                fields.insert(
                    field_name.clone(),
                    FieldMeta {
                        is_public: exposed,
                        is_option: type_is_option(&field.ty),
                    },
                );
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

#[instrument(level = "debug", skip(ty), ret)]
fn type_is_option(ty: &syn::Type) -> bool {
    let syn::Type::Path(type_path) = ty else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Option")
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
