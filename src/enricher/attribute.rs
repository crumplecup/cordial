use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, ImplItem, Item, ItemFn, ItemImpl, ItemMod, Meta, Path as SynPath, Type};

use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{EdgeKind, IrMut, ItemKind, NodeKind, NodeWeight};
use crate::loader::SourceLoadView;
use crate::objects::FileSpan;
use crate::session::SessionView;

use tracing::instrument;
/// Materializes `Attribute` nodes linked to items via [`EdgeKind::HasAttr`].
#[derive(Debug, Default, Clone, Copy)]
pub struct AttributeEnricher;

impl AttributeEnricher {
    /// Stable identifier for `AttributeEnricher`.
    pub const ID: &'static str = "attribute";
}

impl IrEnricher for AttributeEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        6
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir;
        let load = view.load;
        let session = view.session;

        let Some(source) = load.as_any().downcast_ref::<SourceLoadView>() else {
            return Ok(());
        };

        for file in &source.files {
            let syntax = syn::parse_file(&file.source).map_err(|err| {
                crate::error::CordialError::syn_parse(file.path.display().to_string(), err)
            })?;
            let module_prefix =
                crate::loader::module_path_from_src_file(&source.src_root, &file.path);
            let mut visitor = AttributeVisitor {
                ir,
                _session: session,
                file: &file.path,
                module_prefix,
                rel_file: file
                    .path
                    .strip_prefix(session.project_root())
                    .unwrap_or(&file.path)
                    .to_string_lossy()
                    .replace('\\', "/"),
                error: None,
            };
            visitor.visit_file(&syntax);
            if let Some(err) = visitor.error {
                return Err(err);
            }
        }
        Ok(())
    }
}

struct AttributeVisitor<'a> {
    ir: &'a mut dyn IrMut,
    _session: &'a dyn SessionView,
    file: &'a Path,
    module_prefix: Vec<String>,
    rel_file: String,
    error: Option<crate::error::CordialError>,
}

impl AttributeVisitor<'_> {
    #[instrument(level = "debug", skip(self))]
    fn qualify(&self, local: &str) -> String {
        if self.module_prefix.is_empty() {
            local.to_string()
        } else {
            format!("{}::{local}", self.module_prefix.join("::"))
        }
    }

    #[instrument(level = "debug", skip(self, path, attrs))]
    fn attach_attrs(&mut self, path: &str, attrs: &[Attribute], line: u32) {
        if self.error.is_some() {
            return;
        }
        let Some(item_id) = self.ir.node_by_path(path) else {
            return;
        };

        let mut instrumented = false;
        for attr in attrs {
            if self.error.is_some() {
                return;
            }
            let attr_path = attr_path_label(attr);
            if is_instrument_attr(attr) {
                instrumented = true;
            }
            let attr_node = match self.ir.insert_node(
                NodeWeight::new(NodeKind::Attribute)
                    .with_name(attr_path.clone())
                    .with_span(FileSpan::new(self.file, line, 1)),
            ) {
                Ok(node) => node,
                Err(err) => {
                    self.error = Some(err);
                    return;
                }
            };
            let _ = self
                .ir
                .set_attr(attr_node, "attr_path", serde_json::Value::String(attr_path));
            let _ = self.ir.set_attr(
                attr_node,
                "meta",
                serde_json::Value::String(attr_meta_string(attr)),
            );
            let _ = self.ir.set_attr(
                attr_node,
                "file",
                serde_json::Value::String(self.rel_file.clone()),
            );
            let _ = self.ir.insert_edge(item_id, attr_node, EdgeKind::HasAttr);
        }

        if self.ir.node(item_id).is_some_and(|node| {
            matches!(node.kind(), NodeKind::Item(ItemKind::Fn))
                && node.attr("function_kind").is_some()
        }) {
            let _ = self.ir.set_attr(
                item_id,
                "instrumented",
                serde_json::Value::Bool(instrumented),
            );
        }
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
                let line = item_fn.span().start().line as u32;
                self.attach_attrs(&self.qualify(&name), &item_fn.attrs, line);
            }
            Item::Struct(item_struct) => {
                let name = item_struct.ident.to_string();
                let line = item_struct.span().start().line as u32;
                self.attach_attrs(&self.qualify(&name), &item_struct.attrs, line);
            }
            Item::Enum(item_enum) => {
                let name = item_enum.ident.to_string();
                let line = item_enum.span().start().line as u32;
                self.attach_attrs(&self.qualify(&name), &item_enum.attrs, line);
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
            let line = item_mod.span().start().line as u32;
            self.attach_attrs(
                &self.qualify(&item_mod.ident.to_string()),
                &item_mod.attrs,
                line,
            );
            return;
        };
        let mut nested = self.module_prefix.clone();
        nested.push(item_mod.ident.to_string());
        self.visit_module_items(items, &nested);
    }

    #[instrument(level = "debug", skip(self, item_impl))]
    fn visit_impl(&mut self, item_impl: &ItemImpl) {
        let self_ty = type_label(&item_impl.self_ty);
        let trait_name = item_impl
            .trait_
            .as_ref()
            .map(|(_, path, _)| syn_path_label(path));
        for impl_item in &item_impl.items {
            let ImplItem::Fn(method) = impl_item else {
                continue;
            };
            let local = if let Some(trait_name) = trait_name.clone() {
                format!("{trait_name}::{}", method.sig.ident)
            } else {
                format!("{self_ty}::{}", method.sig.ident)
            };
            let line = method.span().start().line as u32;
            self.attach_attrs(&self.qualify(&local), &method.attrs, line);
        }
    }
}

impl<'ast> Visit<'ast> for AttributeVisitor<'_> {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let name = node.sig.ident.to_string();
        let line = node.span().start().line as u32;
        self.attach_attrs(&self.qualify(&name), &node.attrs, line);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.visit_impl(node);
    }
}

#[instrument(level = "debug", skip(ir), err(level = "warn"))]
pub(crate) fn resolve_parent(ir: &dyn IrMut, context: &str) -> CordialResult<crate::ir::NodeId> {
    if context == "<crate>" {
        return ir.root();
    }

    if let Some(node) = ir.node_by_path(context) {
        return Ok(node);
    }

    if let Some((module, _rest)) = context.rsplit_once("::")
        && let Some(node) = ir.node_by_path(module)
    {
        return Ok(node);
    }

    ir.root()
}

/// Crate root directory for a source-loaded member, given its `src/` root.
#[instrument(level = "debug", skip(source, session))]
pub(crate) fn member_crate_root(source: &SourceLoadView, session: &dyn SessionView) -> PathBuf {
    source
        .src_root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| session.project_root().to_path_buf())
}

/// Resolves a scan-recorded (possibly relative) source path against the
/// project root, for findings whose scan step ran outside session context.
#[instrument(level = "debug", skip(session, path))]
pub(crate) fn resolve_source_path(session: &dyn SessionView, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        session.project_root().join(path)
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
        Meta::List(list) => format!("{}({})", syn_path_label(&list.path), list.tokens),
        Meta::NameValue(value) => format!("{} = …", syn_path_label(&value.path)),
    }
}

/// `true` for `#[instrument]`, `#[tracing::instrument]`, and the same
/// wrapped in `#[cfg_attr(<predicate>, ...)]` -- apply writes the gated
/// form for verifier crates (`not(kani)`, etc.), and a later quality
/// run has to treat that as already instrumented or every gated
/// function stays an open missing-instrument finding forever.
#[instrument(level = "trace", skip(attr))]
pub(crate) fn is_instrument_attr(attr: &Attribute) -> bool {
    match &attr.meta {
        Meta::Path(path) => path_is_instrument(path),
        Meta::List(list) => path_is_instrument(&list.path) || cfg_attr_inner_is_instrument(list),
        Meta::NameValue(value) => path_is_instrument(&value.path),
    }
}

/// `true` when `attr` is `#[cfg_attr(.., instrument)]` / `tracing::instrument`
/// — the gated form apply writes for verifier crates. Bare `#[instrument]`
/// is not gated: a Kani (etc.) build will still expand it.
#[instrument(level = "trace", skip(attr), ret)]
pub(crate) fn is_gated_instrument_attr(attr: &Attribute) -> bool {
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

#[instrument(level = "trace", skip(list), ret)]
fn cfg_attr_inner_is_instrument(list: &syn::MetaList) -> bool {
    if !list.path.is_ident("cfg_attr") {
        return false;
    }
    cfg_attr_inner_path(&list.tokens).is_some_and(|path| path_is_instrument(&path))
}

/// Path of `cfg_attr`'s inner attribute (`tracing::instrument` in
/// `cfg_attr(not(kani), tracing::instrument(level = "debug"))`).
#[instrument(level = "trace", skip(tokens))]
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

#[instrument(level = "trace", skip(attrs))]
pub(crate) fn is_cfg_test(attrs: &[Attribute]) -> bool {
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

#[instrument(level = "debug", skip(ty))]
fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => syn_path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
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
