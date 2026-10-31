//! Attribute projection from the shared frontend during migration.

use super::{SourceAttribute, SourceAttributedItem, SourceCrateLints, SourceFileAttributes};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Attribute, File, ImplItem, Item, ItemImpl, ItemMod, Meta, MetaList, Path as SynPath, Token,
    Type,
};
use tracing::instrument;

impl SourceFileAttributes {
    #[instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "SourceFileAttributes")]
    pub(crate) fn collect(file: &crate::loader::SharedParsedFile) -> Self {
        let mut visitor = AttributeVisitor {
            items: Vec::new(),
            module_prefix: Vec::new(),
        };
        visitor.visit_file(file.syntax());
        Self {
            source_digest: file.source_digest(),
            insert_at: file
                .syntax()
                .items
                .first()
                .map(|item| item.span().start().line.saturating_sub(1))
                .unwrap_or_else(|| file.source().lines().count()),
            lints: inspect_file(file.syntax()),
            assume_specification: file.source().contains("assume_specification"),
            items: visitor.items,
        }
    }
}

#[instrument(level = "debug", skip(attr))]
fn attribute(attr: &Attribute) -> SourceAttribute {
    use quote::ToTokens;
    SourceAttribute {
        path: attr_path_label(attr),
        meta: attr_meta_string(attr),
        range: (!attr
            .span()
            .source_text()
            .is_some_and(|text| text.starts_with("//")))
        .then(|| attr.span().byte_range().into()),
        instrument: is_instrument_attr(attr),
        instrument_syntax: super::instrument::collect(
            &super::instrument::compatibility_tokens(attr.meta.to_token_stream()),
            None,
        ),
    }
}

#[instrument(level = "debug", skip(tokens))]
fn render_tokens(tokens: &proc_macro2::TokenStream) -> String {
    super::render_tokens(token_parts(tokens))
}

#[instrument(level = "debug", skip(tokens))]
fn token_parts(tokens: &proc_macro2::TokenStream) -> Vec<String> {
    tokens
        .clone()
        .into_iter()
        .flat_map(|token| match token {
            proc_macro2::TokenTree::Group(group) => {
                let (open, close) = match group.delimiter() {
                    proc_macro2::Delimiter::Parenthesis => ("(", ")"),
                    proc_macro2::Delimiter::Brace => ("{", "}"),
                    proc_macro2::Delimiter::Bracket => ("[", "]"),
                    proc_macro2::Delimiter::None => ("", ""),
                };
                let mut parts = Vec::new();
                if !open.is_empty() {
                    parts.push(open.into());
                }
                parts.extend(token_parts(&group.stream()));
                if !close.is_empty() {
                    parts.push(close.into());
                }
                parts
            }
            token => vec![token.to_string()],
        })
        .collect()
}

struct AttributeVisitor {
    items: Vec<SourceAttributedItem>,
    module_prefix: Vec<String>,
}

impl AttributeVisitor {
    #[instrument(level = "debug", skip(self))]
    fn qualify(&self, local: &str) -> String {
        if self.module_prefix.is_empty() {
            local.to_string()
        } else {
            format!("{}::{local}", self.module_prefix.join("::"))
        }
    }

    #[instrument(level = "debug", skip(self, path, attrs, span))]
    fn attach_attrs(&mut self, path: &str, attrs: &[Attribute], span: proc_macro2::Span) {
        self.items.push(SourceAttributedItem {
            path: path.to_string(),
            range: span.byte_range().into(),
            line: span.start().line as u32,
            attributes: attrs.iter().map(attribute).collect(),
        });
    }

    #[instrument(level = "debug", skip(self, items))]
    fn visit_module_items(&mut self, items: &[Item], module_prefix: &[String]) {
        let prev = self.module_prefix.clone();
        self.module_prefix = module_prefix.to_vec();
        for item in items {
            self.visit_item(item);
        }
        self.module_prefix = prev;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item(&mut self, item: &Item) {
        match item {
            Item::Fn(item_fn) => {
                let name = item_fn.sig.ident.to_string();
                let span = item_fn.span();
                self.attach_attrs(&self.qualify(&name), &item_fn.attrs, span);
            }
            Item::Struct(item_struct) => {
                let name = item_struct.ident.to_string();
                let span = item_struct.span();
                self.attach_attrs(&self.qualify(&name), &item_struct.attrs, span);
            }
            Item::Enum(item_enum) => {
                let name = item_enum.ident.to_string();
                let span = item_enum.span();
                self.attach_attrs(&self.qualify(&name), &item_enum.attrs, span);
            }
            Item::Mod(item_mod) => self.visit_mod(item_mod),
            Item::Impl(item_impl) => self.visit_impl(item_impl),
            _ => {}
        }
    }

    #[instrument(level = "debug", skip(self, item_mod))]
    fn visit_mod(&mut self, item_mod: &ItemMod) {
        if is_cfg_test(&item_mod.attrs) {
            return;
        }
        let Some((_, items)) = &item_mod.content else {
            let span = item_mod.span();
            self.attach_attrs(
                &self.qualify(&item_mod.ident.to_string()),
                &item_mod.attrs,
                span,
            );
            return;
        };
        let mut nested = self.module_prefix.clone();
        nested.push(item_mod.ident.to_string());
        self.visit_module_items(items, &nested);
    }

    #[instrument(level = "debug", skip(self, item_impl))]
    fn visit_impl(&mut self, item_impl: &ItemImpl) {
        // Must match `tracing`'s `impl_method_local_name` exactly, or an
        // existing `#[instrument]` on a trait impl method attaches to no
        // node (the tracing enricher records it under
        // `<Type as Trait>::method`, generics kept).
        let self_ty = self_type_key(&item_impl.self_ty);
        let trait_name = item_impl
            .trait_
            .as_ref()
            .map(|(path, _)| syn_path_label(path));
        for impl_item in &item_impl.items {
            let ImplItem::Fn(method) = impl_item else {
                continue;
            };
            let local = if let Some(trait_name) = trait_name.clone() {
                format!("<{self_ty} as {trait_name}>::{}", method.sig.ident)
            } else {
                format!("{self_ty}::{}", method.sig.ident)
            };
            let span = method.span();
            self.attach_attrs(&self.qualify(&local), &method.attrs, span);
        }
    }
}

impl<'ast> Visit<'ast> for AttributeVisitor {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item(&mut self, node: &'ast Item) {
        AttributeVisitor::visit_item(self, node);
    }
}

#[instrument(level = "debug", skip(attr))]
fn attr_path_label(attr: &Attribute) -> String {
    match &attr.meta {
        Meta::Path(path) => syn_path_to_string(path),
        Meta::List(list) => syn_path_to_string(&list.path),
        Meta::NameValue(value) => syn_path_to_string(&value.path),
    }
}

#[instrument(level = "debug", skip(attr))]
fn attr_meta_string(attr: &Attribute) -> String {
    match &attr.meta {
        Meta::Path(path) => syn_path_label(path),
        Meta::List(list) => format!(
            "{}({})",
            syn_path_label(&list.path),
            render_tokens(&list.tokens)
        ),
        Meta::NameValue(value) => format!("{} = …", syn_path_label(&value.path)),
    }
}

/// `true` for `#[instrument]`, `#[tracing::instrument]`, and the same
/// wrapped in `#[cfg_attr(<predicate>, ...)]` -- apply writes the gated
/// form for verifier crates (`not(kani)`, etc.), and a later quality
/// run has to treat that as already instrumented or every gated
/// function stays an open missing-instrument finding forever.
#[instrument(level = "trace", skip(attr))]
pub fn is_instrument_attr(attr: &Attribute) -> bool {
    match &attr.meta {
        Meta::Path(path) => path_is_instrument(path),
        Meta::List(list) => path_is_instrument(&list.path) || cfg_attr_inner_is_instrument(list),
        Meta::NameValue(value) => path_is_instrument(&value.path),
    }
}

/// `true` when `attr` is `#[cfg_attr(.., instrument)]` / `tracing::instrument`
/// — the gated form apply writes for verifier crates. Bare `#[instrument]`
/// is not gated: a Kani (etc.) build will still expand it.
#[instrument(level = "trace", skip(attr))]
pub fn is_gated_instrument_attr(attr: &Attribute) -> bool {
    match &attr.meta {
        Meta::List(list) => cfg_attr_inner_is_instrument(list),
        _ => false,
    }
}

#[instrument(level = "debug", skip(path))]
fn path_is_instrument(path: &SynPath) -> bool {
    if path.is_ident("instrument") {
        return true;
    }
    path.segments.len() == 2
        && path.segments[0].ident == "tracing"
        && path.segments[1].ident == "instrument"
}

#[instrument(level = "debug", skip(list))]
fn cfg_attr_inner_is_instrument(list: &syn::MetaList) -> bool {
    if !list.path.is_ident("cfg_attr") {
        return false;
    }
    cfg_attr_inner_path(&list.tokens).is_some_and(|path| path_is_instrument(&path))
}

/// Path of `cfg_attr`'s inner attribute (`tracing::instrument` in
/// `cfg_attr(not(kani), tracing::instrument(level = "debug"))`).
#[instrument(level = "debug", skip(tokens))]
fn cfg_attr_inner_path(tokens: &proc_macro2::TokenStream) -> Option<SynPath> {
    let mut seen_comma = false;
    let mut inner = Vec::new();
    for tree in tokens.clone() {
        match &tree {
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == ',' && !seen_comma => {
                seen_comma = true;
            }
            _ if seen_comma => inner.push(tree),
            _ => {}
        }
    }
    if !seen_comma {
        return None;
    }
    let path_tokens: proc_macro2::TokenStream = inner
        .into_iter()
        .take_while(|tree| !matches!(tree, proc_macro2::TokenTree::Group(_)))
        .collect();
    syn::parse2(path_tokens).ok()
}

/// `true` when `attrs` carry `#[cfg(test)]`, so a scan can skip test-only code.
#[instrument(level = "trace", skip(attrs))]
pub fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let Meta::List(list) = &attr.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        list.tokens.to_string().replace(' ', "") == "test"
    })
}

/// A `::`-free rendering of a type that keeps its generic arguments.
/// Mirrors `tracing`'s `scan::self_type_key` (duplicated, following this
/// module's own local-`type_label` convention) so the two enrichers key
/// impl-method nodes identically.
#[instrument(level = "debug", skip(ty))]
fn self_type_key(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => {
            let Some(segment) = type_path.path.segments.last() else {
                return "?".to_string();
            };
            let base = segment.ident.to_string();
            let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
                return base;
            };
            let rendered: Vec<String> = args
                .args
                .iter()
                .filter_map(|arg| match arg {
                    syn::GenericArgument::Type(inner) => Some(self_type_key(inner)),
                    syn::GenericArgument::Const(expr) => Some(const_arg_label(expr)),
                    _ => None,
                })
                .collect();
            if rendered.is_empty() {
                base
            } else {
                format!("{base}<{}>", rendered.join(", "))
            }
        }
        Type::Reference(reference) => {
            let inner = self_type_key(&reference.elem);
            if reference.mutability.is_some() {
                format!("&mut {inner}")
            } else {
                format!("&{inner}")
            }
        }
        Type::Ptr(ptr) => {
            let inner = self_type_key(&ptr.elem);
            if matches!(ptr.mutability, syn::PointerMutability::Mut(_)) {
                format!("*mut {inner}")
            } else {
                format!("*const {inner}")
            }
        }
        Type::Tuple(tuple) => {
            let parts: Vec<String> = tuple.elems.iter().map(self_type_key).collect();
            format!("({})", parts.join(", "))
        }
        Type::Slice(slice) => format!("[{}]", self_type_key(&slice.elem)),
        Type::Array(array) => {
            format!(
                "[{}; {}]",
                self_type_key(&array.elem),
                const_arg_label(&array.len)
            )
        }
        Type::Paren(paren) => self_type_key(&paren.elem),
        Type::Group(group) => self_type_key(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(expr))]
fn const_arg_label(expr: &syn::Expr) -> String {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(int),
            ..
        }) => int.base10_digits().to_string(),
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Bool(b),
            ..
        }) => b.value.to_string(),
        syn::Expr::Path(path) => syn_path_label(&path.path),
        _ => "_".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
fn syn_path_label(path: &SynPath) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| "?".to_string())
}

#[instrument(level = "debug", skip(path))]
fn syn_path_to_string(path: &SynPath) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

#[instrument(level = "debug", skip(file))]
fn inspect_file(file: &File) -> SourceCrateLints {
    let mut presence = SourceCrateLints::default();
    for attr in &file.attrs {
        apply_attr(attr, &mut presence);
    }
    presence
}

#[instrument(level = "debug", skip(attr, presence))]
fn apply_attr(attr: &Attribute, presence: &mut SourceCrateLints) {
    match &attr.meta {
        Meta::List(list) if is_lint_level(&list.path, "forbid") => {
            apply_level("forbid", list, presence);
        }
        Meta::List(list) if is_lint_level(&list.path, "deny") => {
            apply_level("deny", list, presence);
        }
        Meta::List(list) if is_lint_level(&list.path, "warn") => {
            apply_level("warn", list, presence);
        }
        Meta::List(list) if list.path.is_ident("cfg_attr") => {
            apply_cfg_attr(list, presence);
        }
        _ => {}
    }
}

#[instrument(level = "trace", skip(path), ret)]
fn is_lint_level(path: &syn::Path, level: &str) -> bool {
    path.is_ident(level)
}

#[instrument(level = "debug", skip(list, presence))]
fn apply_level(level: &str, list: &MetaList, presence: &mut SourceCrateLints) {
    for name in nested_lint_names(list) {
        match (level, name.as_str()) {
            ("forbid", "unsafe_code") => presence.forbid_unsafe_code = true,
            ("forbid" | "deny" | "warn", "missing_docs") => {
                presence.missing_docs_at_least_warn = true;
            }
            _ => {}
        }
    }
}

#[instrument(level = "debug", skip(list, presence))]
fn apply_cfg_attr(list: &MetaList, presence: &mut SourceCrateLints) {
    let Ok(nested) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated) else {
        return;
    };
    for meta in nested.into_iter().skip(1) {
        let Meta::List(inner) = meta else {
            continue;
        };
        if inner.path.is_ident("forbid") {
            apply_level("forbid", &inner, presence);
        } else if inner.path.is_ident("deny") {
            apply_level("deny", &inner, presence);
        } else if inner.path.is_ident("warn") {
            apply_level("warn", &inner, presence);
        }
    }
}

#[instrument(level = "debug", skip(list))]
fn nested_lint_names(list: &MetaList) -> Vec<String> {
    let Ok(nested) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated) else {
        return Vec::new();
    };
    nested
        .into_iter()
        .filter_map(|meta| match meta {
            Meta::Path(path) => path.segments.last().map(|seg| seg.ident.to_string()),
            Meta::List(inner) => inner.path.segments.last().map(|seg| seg.ident.to_string()),
            Meta::NameValue(nv) => nv.path.segments.last().map(|seg| seg.ident.to_string()),
        })
        .collect()
}
