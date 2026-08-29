//! syn-based scan for `#[cfg(...)]` predicates scattered across many item
//! kinds in one file, instead of gated once at a `mod` declaration.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Attribute, Field, ImplItemConst, ImplItemFn, ImplItemType, ItemConst, ItemEnum, ItemFn,
    ItemImpl, ItemMod, ItemStatic, ItemStruct, ItemTrait, ItemType, ItemUse, Meta, TraitItemConst,
    TraitItemFn, TraitItemType, Type, Variant,
};

use crate::error::CordialResult;
use crate::loader::module_path_from_src_file;

use super::types::{CfgScatterGroup, CfgScatterThresholds, CfgSiteKind, CfgSiteOccurrence};

use tracing::instrument;
#[instrument(level = "debug", skip(thresholds), err(level = "warn"))]
pub fn scan_source_tree(
    src_root: &Path,
    crate_root: &Path,
    thresholds: CfgScatterThresholds,
) -> CordialResult<Vec<CfgScatterGroup>> {
    let mut groups = Vec::new();
    if !src_root.is_dir() {
        return Ok(groups);
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
        groups.extend(scan_rust_source(&source, path, src_root, crate_root)?);
    }

    groups.retain(|group| group.is_scatter(&thresholds));
    groups.sort_by(|a, b| {
        b.non_field_count()
            .cmp(&a.non_field_count())
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.predicate.cmp(&b.predicate))
    });

    Ok(groups)
}

/// Scan one Rust source file and return records.
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<CfgScatterGroup>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut rel_file = file.to_path_buf();
    if let Ok(rel) = rel_file.strip_prefix(crate_root) {
        rel_file = rel.to_path_buf();
    }
    let mut visitor = CfgScatterVisitor {
        file: rel_file,
        module_prefix,
        impl_type: None,
        fn_stack: Vec::new(),
        occurrences: BTreeMap::new(),
    };
    visitor.visit_file(&syntax);
    Ok(visitor.into_groups())
}

struct CfgScatterVisitor {
    file: PathBuf,
    module_prefix: Vec<String>,
    impl_type: Option<String>,
    fn_stack: Vec<String>,
    /// Keyed by normalized `cfg(...)` predicate text.
    occurrences: BTreeMap<String, Vec<CfgSiteOccurrence>>,
}

impl CfgScatterVisitor {
    #[instrument(level = "debug", skip(self))]
    fn into_groups(self) -> Vec<CfgScatterGroup> {
        self.occurrences
            .into_iter()
            .map(|(predicate, occurrences)| CfgScatterGroup {
                file: self.file.clone(),
                predicate,
                occurrences,
            })
            .collect()
    }

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

    #[instrument(level = "debug", skip(self, attrs, kind, snippet))]
    fn check_attrs(&mut self, attrs: &[Attribute], kind: CfgSiteKind, snippet: impl Into<String>) {
        let line = attrs
            .first()
            .map(|attr| attr.span().start().line as u32)
            .unwrap_or(0);
        let snippet = snippet.into();
        let context = self.site_context();
        for predicate in cfg_predicates(attrs) {
            self.occurrences
                .entry(predicate)
                .or_default()
                .push(CfgSiteOccurrence {
                    kind,
                    context: context.clone(),
                    line,
                    snippet: snippet.clone(),
                });
        }
    }

    #[instrument(level = "debug", skip(self, items))]
    fn visit_module_items(&mut self, items: &[syn::Item], module_prefix: &[String]) {
        let prev_prefix = self.module_prefix.clone();
        self.module_prefix = module_prefix.to_vec();
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.module_prefix = prev_prefix;
    }

    #[instrument(level = "debug", skip(self, item_mod))]
    fn visit_mod(&mut self, item_mod: &ItemMod) {
        // Deliberately not scanned: gating the whole module at the `mod`
        // declaration is the recommended pattern, not the antipattern.
        let Some((_, items)) = &item_mod.content else {
            return;
        };
        let mut nested = self.module_prefix.clone();
        nested.push(item_mod.ident.to_string());
        self.visit_module_items(items, &nested);
    }
}

impl<'ast> Visit<'ast> for CfgScatterVisitor {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::Fn,
            format!("fn {}", node.sig.ident),
        );
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::Struct,
            format!("struct {}", node.ident),
        );
        syn::visit::visit_item_struct(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::Enum,
            format!("enum {}", node.ident),
        );
        syn::visit::visit_item_enum(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::Trait,
            format!("trait {}", node.ident),
        );
        syn::visit::visit_item_trait(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_const(&mut self, node: &'ast ItemConst) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::Const,
            format!("const {}", node.ident),
        );
        syn::visit::visit_item_const(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_static(&mut self, node: &'ast ItemStatic) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::Static,
            format!("static {}", node.ident),
        );
        syn::visit::visit_item_static(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_type(&mut self, node: &'ast ItemType) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::TypeAlias,
            format!("type {}", node.ident),
        );
        syn::visit::visit_item_type(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        self.check_attrs(&node.attrs, CfgSiteKind::Use, "use ...".to_string());
        syn::visit::visit_item_use(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let label = format!("impl {}", type_label(&node.self_ty));
        self.check_attrs(&node.attrs, CfgSiteKind::Impl, label.clone());
        let prev = self.impl_type.clone();
        self.impl_type = Some(type_label(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        self.impl_type = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::ImplFn,
            format!("fn {}", node.sig.ident),
        );
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_const(&mut self, node: &'ast ImplItemConst) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::Const,
            format!("const {}", node.ident),
        );
        syn::visit::visit_impl_item_const(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_type(&mut self, node: &'ast ImplItemType) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::TypeAlias,
            format!("type {}", node.ident),
        );
        syn::visit::visit_impl_item_type(self, node);
    }

    /// Default methods inside a `trait { ... }` body — the counterpart to
    /// [`Self::visit_impl_item_fn`] that was previously missing, which made
    /// `#[cfg(...)]` scattered across trait default methods invisible to
    /// this scanner (see `docs/planning/cfg-scatter-etiquette.md`).
    #[instrument(level = "debug", skip(self, node))]
    fn visit_trait_item_fn(&mut self, node: &'ast TraitItemFn) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::TraitFn,
            format!("fn {}", node.sig.ident),
        );
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_trait_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_trait_item_const(&mut self, node: &'ast TraitItemConst) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::Const,
            format!("const {}", node.ident),
        );
        syn::visit::visit_trait_item_const(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_trait_item_type(&mut self, node: &'ast TraitItemType) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::TypeAlias,
            format!("type {}", node.ident),
        );
        syn::visit::visit_trait_item_type(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_field(&mut self, node: &'ast Field) {
        let name = node
            .ident
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "_".to_string());
        self.check_attrs(&node.attrs, CfgSiteKind::Field, format!("field {name}"));
        syn::visit::visit_field(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_variant(&mut self, node: &'ast Variant) {
        self.check_attrs(
            &node.attrs,
            CfgSiteKind::Variant,
            format!("variant {}", node.ident),
        );
        syn::visit::visit_variant(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        self.check_attrs(&node.attrs, CfgSiteKind::Arm, "match arm".to_string());
        syn::visit::visit_arm(self, node);
    }
}

/// Extract normalized `cfg(...)` predicate text from every `#[cfg(...)]`
/// attribute in `attrs` (there is normally at most one, but attributes can
/// repeat). `#[cfg(test)]` is excluded — it's a build-mode switch, not a
/// feature-flag antipattern candidate.
#[instrument(level = "debug", skip(attrs))]
fn cfg_predicates(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|attr| {
            let Meta::List(list) = &attr.meta else {
                return None;
            };
            if !list.path.is_ident("cfg") {
                return None;
            }
            let normalized = normalize_cfg_tokens(&list.tokens.to_string());
            if normalized == "test" {
                return None;
            }
            Some(normalized)
        })
        .collect()
}

#[instrument(level = "debug")]
fn normalize_cfg_tokens(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" :: ", "::")
        .replace("( ", "(")
        .replace(" )", ")")
        .replace(" ,", ",")
}

#[instrument(level = "debug", skip(ty))]
fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
fn path_label(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}
