//! Universal `#[cfg(...)]`/`#[cfg_attr(...)]` name scan: every cfg *name*
//! mentioned anywhere in a file, via `syn::visit::Visit::visit_attribute`.
//!
//! Unlike `cfg_scatter`'s per-item-kind classification, this scanner doesn't
//! care what kind of item a `cfg` attribute sits on — every generated
//! item-kind visitor in `syn::visit` calls `visit_attribute` for its own
//! `.attrs` first, so overriding just that one method (plus a handful of
//! context-tracking overrides, for a readable label) reaches fields,
//! variants, statements, match arms, and items uniformly, including a
//! `#[cfg(...)]` directly on a `mod` declaration — deliberately *not*
//! excluded here the way `cfg_scatter` excludes it: an undeclared name is
//! just as real a bug on a module gate as anywhere else.

use std::path::{Path, PathBuf};

use quote::ToTokens;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Attribute, ImplItemConst, ImplItemFn, ImplItemType, ItemConst, ItemEnum, ItemFn, ItemImpl,
    ItemMod, ItemStatic, ItemStruct, ItemTrait, ItemType, Meta, Path as SynPath, Token,
    TraitItemConst, TraitItemFn, TraitItemType, Type,
};

use crate::error::CordialResult;
use crate::loader::module_path_from_src_file;

use tracing::instrument;
/// One cfg name mentioned by a `#[cfg(...)]`/`#[cfg_attr(...)]` attribute.
/// A single attribute mentioning several names (e.g. `any(kani, creusot)`)
/// produces one occurrence per name.
#[derive(Debug, Clone)]
pub struct CfgNameOccurrence {
    pub name: String,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
}

/// Scan one Rust source file and return records.
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<CfgNameOccurrence>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut rel_file = file.to_path_buf();
    if let Ok(rel) = rel_file.strip_prefix(crate_root) {
        rel_file = rel.to_path_buf();
    }
    let mut visitor = CfgNameVisitor {
        file: rel_file,
        module_prefix,
        item_stack: Vec::new(),
        occurrences: Vec::new(),
    };
    visitor.visit_file(&syntax);
    Ok(visitor.occurrences)
}

struct CfgNameVisitor {
    file: PathBuf,
    module_prefix: Vec<String>,
    item_stack: Vec<String>,
    occurrences: Vec<CfgNameOccurrence>,
}

impl CfgNameVisitor {
    #[instrument(level = "debug", skip(self))]
    fn context(&self) -> String {
        let mut parts = self.module_prefix.clone();
        parts.extend(self.item_stack.iter().cloned());
        if parts.is_empty() {
            "<crate>".to_string()
        } else {
            parts.join("::")
        }
    }

    #[instrument(level = "debug", skip(self, body))]
    fn with_item<T>(&mut self, label: String, body: impl FnOnce(&mut Self) -> T) -> T {
        self.item_stack.push(label);
        let result = body(self);
        self.item_stack.pop();
        result
    }
}

impl<'ast> Visit<'ast> for CfgNameVisitor {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        let Some((_, items)) = &node.content else {
            return;
        };
        let prev = self.module_prefix.clone();
        self.module_prefix.push(node.ident.to_string());
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.module_prefix = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let label = format!("fn {}", node.sig.ident);
        self.with_item(label, |this| syn::visit::visit_item_fn(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        let label = format!("struct {}", node.ident);
        self.with_item(label, |this| syn::visit::visit_item_struct(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        let label = format!("enum {}", node.ident);
        self.with_item(label, |this| syn::visit::visit_item_enum(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        let label = format!("trait {}", node.ident);
        self.with_item(label, |this| syn::visit::visit_item_trait(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let label = format!("impl {}", type_label(&node.self_ty));
        self.with_item(label, |this| syn::visit::visit_item_impl(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_const(&mut self, node: &'ast ItemConst) {
        let label = format!("const {}", node.ident);
        self.with_item(label, |this| syn::visit::visit_item_const(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_static(&mut self, node: &'ast ItemStatic) {
        let label = format!("static {}", node.ident);
        self.with_item(label, |this| syn::visit::visit_item_static(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_type(&mut self, node: &'ast ItemType) {
        let label = format!("type {}", node.ident);
        self.with_item(label, |this| syn::visit::visit_item_type(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        let label = format!("fn {}", node.sig.ident);
        self.with_item(label, |this| syn::visit::visit_impl_item_fn(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_const(&mut self, node: &'ast ImplItemConst) {
        let label = format!("const {}", node.ident);
        self.with_item(label, |this| syn::visit::visit_impl_item_const(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_type(&mut self, node: &'ast ImplItemType) {
        let label = format!("type {}", node.ident);
        self.with_item(label, |this| syn::visit::visit_impl_item_type(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_trait_item_fn(&mut self, node: &'ast TraitItemFn) {
        let label = format!("fn {}", node.sig.ident);
        self.with_item(label, |this| syn::visit::visit_trait_item_fn(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_trait_item_const(&mut self, node: &'ast TraitItemConst) {
        let label = format!("const {}", node.ident);
        self.with_item(label, |this| syn::visit::visit_trait_item_const(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_trait_item_type(&mut self, node: &'ast TraitItemType) {
        let label = format!("type {}", node.ident);
        self.with_item(label, |this| syn::visit::visit_trait_item_type(this, node));
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_attribute(&mut self, node: &'ast Attribute) {
        let names = cfg_names(node);
        if names.is_empty() {
            return;
        }
        let context = self.context();
        let line = node.span().start().line as u32;
        let snippet = attr_snippet(node);
        for name in names {
            self.occurrences.push(CfgNameOccurrence {
                name,
                context: context.clone(),
                file: self.file.clone(),
                line,
                snippet: snippet.clone(),
            });
        }
    }
}

/// Every cfg name a `#[cfg(...)]`/`#[cfg_attr(...)]` attribute mentions,
/// walking `all()`/`any()`/`not()` combinators recursively. For
/// `cfg_attr(predicate, attr...)` only the leading `predicate` is a cfg
/// expression — the rest are the attributes to splice in, not cfg names.
#[instrument(level = "debug", skip(attr))]
fn cfg_names(attr: &Attribute) -> Vec<String> {
    let Meta::List(list) = &attr.meta else {
        return Vec::new();
    };
    let is_cfg = list.path.is_ident("cfg");
    let is_cfg_attr = list.path.is_ident("cfg_attr");
    if !is_cfg && !is_cfg_attr {
        return Vec::new();
    }
    let Ok(metas) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated) else {
        return Vec::new();
    };
    if is_cfg {
        metas.iter().flat_map(collect_predicate_names).collect()
    } else {
        metas
            .iter()
            .take(1)
            .flat_map(collect_predicate_names)
            .collect()
    }
}

#[instrument(level = "debug", skip(meta))]
fn collect_predicate_names(meta: &Meta) -> Vec<String> {
    match meta {
        Meta::Path(path) => vec![path_name(path)],
        Meta::NameValue(name_value) => vec![path_name(&name_value.path)],
        Meta::List(list) => {
            let combinator = path_name(&list.path);
            if matches!(combinator.as_str(), "all" | "any" | "not") {
                let Ok(inner) =
                    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                else {
                    return Vec::new();
                };
                inner.iter().flat_map(collect_predicate_names).collect()
            } else {
                // Not real cfg syntax in practice, but don't silently drop
                // it -- surface the combinator's own path as a name rather
                // than losing the occurrence entirely.
                vec![combinator]
            }
        }
    }
}

#[instrument(level = "debug", skip(path))]
fn path_name(path: &SynPath) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_default()
}

#[instrument(level = "debug", skip(attr))]
fn attr_snippet(attr: &Attribute) -> String {
    let text = attr.to_token_stream().to_string();
    if text.chars().count() > 120 {
        format!("{}…", text.chars().take(120).collect::<String>())
    } else {
        text
    }
}

#[instrument(level = "debug", skip(ty))]
fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => path_name(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}
