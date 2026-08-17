//! syn-based scan for antipattern probes (`Box<dyn Error>`, `Result<_, String>`, `&'static` struct fields, …).

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Fields, FnArg, ItemEnum, ItemFn, ItemImpl, ItemMod, ItemStruct, ItemTrait, ItemType, Pat,
    PathArguments, Signature, TraitItem, Type, TypeParamBound, TypePath, TypeTraitObject,
};

use crate::enricher::is_cfg_test;
use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, path_has_fixtures};

use super::types::{AntipatternRuleId, AntipatternSiteRecord};

pub fn scan_source_tree(
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let mut findings = Vec::new();
    if !tree_root.is_dir() {
        return Ok(findings);
    }

    for entry in walkdir::WalkDir::new(tree_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") || path_has_fixtures(path, crate_root) {
            continue;
        }
        let source = std::fs::read_to_string(path)?;
        findings.extend(scan_rust_source(&source, path, tree_root, crate_root)?);
    }

    Ok(findings)
}

pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = AntipatternScanVisitor {
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        impl_type: None,
        fn_stack: Vec::new(),
        in_trait_definition: false,
        findings: Vec::new(),
    };
    visitor.visit_file(&syntax);
    Ok(visitor.findings)
}

struct AntipatternScanVisitor {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    impl_type: Option<String>,
    fn_stack: Vec<String>,
    in_trait_definition: bool,
    findings: Vec<AntipatternSiteRecord>,
}

struct UnusedArgBinding {
    line: u32,
    snippet: String,
}

impl AntipatternScanVisitor {
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

    fn rel_file(&self) -> PathBuf {
        self.file
            .strip_prefix(&self.crate_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| self.file.clone())
    }

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

    fn check_adt_field(&mut self, owner: &str, field_name: &str, ty: &Type) {
        if !type_contains_static_lifetime_ref(ty) {
            return;
        }
        self.findings.push(AntipatternSiteRecord {
            rule_id: AntipatternRuleId::StructStaticRef001,
            context: self.adt_field_context(owner, field_name),
            file: self.rel_file(),
            line: ty.span().start().line as u32,
            snippet: static_ref_snippet(ty),
        });
    }

    fn check_enum_variant(&mut self, enum_name: &str, variant: &syn::Variant) {
        let owner = format!("{enum_name}::{}", variant.ident);
        match &variant.fields {
            Fields::Named(fields) => self.check_named_fields(&owner, fields),
            Fields::Unnamed(fields) => self.check_unnamed_fields(&owner, fields),
            Fields::Unit => {}
        }
    }

    fn adt_field_context(&self, owner: &str, field_name: &str) -> String {
        let mut parts = self.module_prefix.clone();
        parts.push(owner.to_string());
        parts.push(field_name.to_string());
        parts.join("::")
    }

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

    fn check_unnamed_fields(&mut self, owner: &str, fields: &syn::FieldsUnnamed) {
        for (index, field) in fields.unnamed.iter().enumerate() {
            self.check_adt_field(owner, &format!("_{index}"), &field.ty);
        }
    }

    fn check_fn_sig(&mut self, sig: &Signature) {
        if self.in_trait_definition {
            return;
        }
        for arg in &sig.inputs {
            let FnArg::Typed(pat_type) = arg else {
                continue;
            };
            for binding in unused_argument_bindings(&pat_type.pat) {
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

    fn visit_module_items(&mut self, items: &[syn::Item], module_prefix: &[String]) {
        let prev_prefix = self.module_prefix.clone();
        self.module_prefix = module_prefix.to_vec();
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.module_prefix = prev_prefix;
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
}

impl<'ast> Visit<'ast> for AntipatternScanVisitor {
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        let owner = node.ident.to_string();
        match &node.fields {
            Fields::Named(fields) => self.check_named_fields(&owner, fields),
            Fields::Unnamed(fields) => self.check_unnamed_fields(&owner, fields),
            Fields::Unit => {}
        }
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        let enum_name = node.ident.to_string();
        for variant in &node.variants {
            self.check_enum_variant(&enum_name, variant);
        }
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_fn_sig(&node.sig);
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
    }

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

    fn visit_item_type(&mut self, node: &'ast ItemType) {
        self.fn_stack.push(node.ident.to_string());
        syn::visit::visit_item_type(self, node);
        self.fn_stack.pop();
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let prev = self.impl_type.clone();
        self.impl_type = Some(type_label(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        self.impl_type = prev;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_fn_sig(&node.sig);
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
    }

    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}

    fn visit_type(&mut self, node: &'ast Type) {
        self.check_box_dyn_error_type(node);
        self.check_string_error_type(node);
        syn::visit::visit_type(self, node);
    }
}

fn type_contains_static_lifetime_ref(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => {
            reference
                .lifetime
                .as_ref()
                .is_some_and(|lifetime| lifetime.ident == "static")
                || type_contains_static_lifetime_ref(&reference.elem)
        }
        Type::Path(type_path) => type_path
            .path
            .segments
            .iter()
            .any(|segment| match &segment.arguments {
                PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| {
                    matches!(arg, syn::GenericArgument::Type(inner) if type_contains_static_lifetime_ref(inner))
                }),
                PathArguments::Parenthesized(args) => args
                    .inputs
                    .iter()
                    .any(type_contains_static_lifetime_ref),
                PathArguments::None => false,
            }),
        Type::Array(array) => type_contains_static_lifetime_ref(&array.elem),
        Type::Slice(slice) => type_contains_static_lifetime_ref(&slice.elem),
        Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .any(type_contains_static_lifetime_ref),
        Type::Paren(paren) => type_contains_static_lifetime_ref(&paren.elem),
        Type::Group(group) => type_contains_static_lifetime_ref(&group.elem),
        Type::Ptr(pointer) => type_contains_static_lifetime_ref(&pointer.elem),
        Type::TraitObject(trait_obj) => trait_obj
            .bounds
            .iter()
            .any(|bound| match bound {
                TypeParamBound::Trait(trait_bound) => trait_bound
                    .path
                    .segments
                    .iter()
                    .any(|segment| match &segment.arguments {
                        PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| {
                            matches!(arg, syn::GenericArgument::Type(inner) if type_contains_static_lifetime_ref(inner))
                        }),
                        _ => false,
                    }),
                _ => false,
            }),
        _ => false,
    }
}

fn static_ref_snippet(ty: &Type) -> String {
    truncate_snippet(&type_label_with_lifetime(ty), 96)
}

fn type_label_with_lifetime(ty: &Type) -> String {
    match ty {
        Type::Reference(reference) => {
            let mut out = String::from("&");
            if let Some(lifetime) = &reference.lifetime {
                out.push('\'');
                out.push_str(&lifetime.ident.to_string());
            }
            if reference.mutability.is_some() {
                out.push_str(" mut");
            }
            out.push(' ');
            out.push_str(&type_label_with_lifetime(&reference.elem));
            out
        }
        Type::Path(type_path) => {
            let mut segments = Vec::new();
            for segment in &type_path.path.segments {
                let mut label = segment.ident.to_string();
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    let inner: Vec<String> = args
                        .args
                        .iter()
                        .filter_map(|arg| match arg {
                            syn::GenericArgument::Type(inner) => {
                                Some(type_label_with_lifetime(inner))
                            }
                            syn::GenericArgument::Lifetime(lifetime) => {
                                Some(format!("'{}", lifetime.ident))
                            }
                            _ => None,
                        })
                        .collect();
                    if !inner.is_empty() {
                        label = format!("{label}<{inner}>", inner = inner.join(", "));
                    }
                }
                segments.push(label);
            }
            segments.join("::")
        }
        Type::Array(array) => format!("[{}; …]", type_label_with_lifetime(&array.elem)),
        Type::Slice(slice) => format!("[{}]", type_label_with_lifetime(&slice.elem)),
        Type::Tuple(tuple) => {
            let inner = tuple
                .elems
                .iter()
                .map(type_label_with_lifetime)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({inner})")
        }
        Type::Paren(paren) => type_label_with_lifetime(&paren.elem),
        Type::Group(group) => type_label_with_lifetime(&group.elem),
        _ => type_label(ty),
    }
}

fn unused_argument_bindings(pat: &Pat) -> Vec<UnusedArgBinding> {
    let mut bindings = Vec::new();
    collect_unused_argument_bindings(pat, &mut bindings);
    bindings
}

fn collect_unused_argument_bindings(pat: &Pat, bindings: &mut Vec<UnusedArgBinding>) {
    match pat {
        Pat::Wild(wild) => bindings.push(UnusedArgBinding {
            line: wild.span().start().line as u32,
            snippet: "_".to_string(),
        }),
        Pat::Ident(ident) if is_unused_argument_ident(&ident.ident) => {
            bindings.push(UnusedArgBinding {
                line: ident.span().start().line as u32,
                snippet: ident.ident.to_string(),
            });
        }
        Pat::Reference(reference) => collect_unused_argument_bindings(&reference.pat, bindings),
        Pat::Type(pat_type) => collect_unused_argument_bindings(&pat_type.pat, bindings),
        Pat::Paren(paren) => collect_unused_argument_bindings(&paren.pat, bindings),
        Pat::Tuple(tuple) => {
            for element in &tuple.elems {
                collect_unused_argument_bindings(element, bindings);
            }
        }
        Pat::TupleStruct(tuple_struct) => {
            for element in &tuple_struct.elems {
                collect_unused_argument_bindings(element, bindings);
            }
        }
        Pat::Struct(pat_struct) => {
            for field in &pat_struct.fields {
                collect_unused_argument_bindings(&field.pat, bindings);
            }
        }
        _ => {}
    }
}

fn is_unused_argument_ident(ident: &syn::Ident) -> bool {
    ident.to_string().starts_with('_')
}

fn result_error_type(ty: &Type) -> Option<&Type> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return None;
    };
    let segment = path.segments.last()?;
    if segment.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let type_args: Vec<&Type> = args
        .args
        .iter()
        .filter_map(|arg| match arg {
            syn::GenericArgument::Type(inner) => Some(inner),
            _ => None,
        })
        .collect();
    type_args.get(1).copied()
}

fn is_stringish_error_type(ty: &Type) -> bool {
    match ty {
        Type::Path(TypePath { path, .. }) => {
            let ident = path
                .segments
                .last()
                .map(|segment| segment.ident.to_string());
            matches!(ident.as_deref(), Some("String") | Some("str"))
        }
        Type::Reference(reference) => is_stringish_error_type(&reference.elem),
        Type::Paren(paren) => is_stringish_error_type(&paren.elem),
        Type::Group(group) => is_stringish_error_type(&group.elem),
        _ => false,
    }
}

fn result_string_error_snippet(ty: &Type) -> String {
    match ty {
        Type::Path(_) => format!(
            "Result<…, {}>",
            type_label(result_error_type(ty).unwrap_or(ty))
        ),
        Type::Paren(paren) => result_string_error_snippet(&paren.elem),
        Type::Group(group) => result_string_error_snippet(&group.elem),
        _ => "Result<…, String>".to_string(),
    }
}

fn box_dyn_error_trait_object(ty: &Type) -> Option<&TypeTraitObject> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return None;
    };
    let segment = path.segments.last()?;
    if segment.ident != "Box" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    let syn::GenericArgument::Type(inner) = &args.args[0] else {
        return None;
    };
    let Type::TraitObject(trait_obj) = inner else {
        return None;
    };
    if !trait_object_has_error_bound(trait_obj) {
        return None;
    }
    Some(trait_obj)
}

fn trait_object_has_error_bound(trait_obj: &TypeTraitObject) -> bool {
    trait_obj.bounds.iter().any(|bound| match bound {
        TypeParamBound::Trait(trait_bound) => trait_bound
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Error"),
        _ => false,
    })
}

fn box_dyn_error_snippet(trait_obj: &TypeTraitObject) -> String {
    let bounds: Vec<String> = trait_obj.bounds.iter().map(trait_bound_label).collect();
    let snippet = format!("Box<dyn {}>", bounds.join(" + "));
    truncate_snippet(&snippet, 96)
}

fn trait_bound_label(bound: &TypeParamBound) -> String {
    match bound {
        TypeParamBound::Trait(trait_bound) => path_label(&trait_bound.path),
        TypeParamBound::Lifetime(lifetime) => lifetime.ident.to_string(),
        TypeParamBound::PreciseCapture(_) => "use<…>".to_string(),
        _ => "?".to_string(),
    }
}

pub(crate) fn truncate_snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
}

fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

fn path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}
