//! Compatibility loader projection; candidate dispatch never receives syntax.
use super::super::{
    ErrorEdgeField,
    attributes::call_site_attribute_is_stable,
    edges::error_edge_evidence,
    from::ErrorFromEvidence,
    helpers::derive_is_nonconstructing,
    result_alias::stable_result_alias,
    source_method::ErrorSourceMethodEvidence,
    try_conversion::{ErrorTryEvidence, ErrorTryScope},
};
use super::{CalleeGuard, ConversionFileFacts, Event, Kind, PathFact};
use crate::etiquettes::internal_error_chain::{
    apply::{
        simple_type_name,
        uses::{macro_has_unhandled_field_use, tokens_access_field},
    },
    type_graph::trait_is_std_error,
};
use crate::loader::SharedParsedFile;
use std::collections::BTreeSet;
use syn::{spanned::Spanned, visit::Visit};

struct Collector<'a> {
    file: &'a SharedParsedFile,
    events: Vec<Event>,
    depth: usize,
    owner: Option<String>,
    error_trait: Option<syn::Path>,
    source_site: Option<usize>,
    returns: Option<BTreeSet<String>>,
    try_evidence: Option<ErrorTryEvidence>,
    guards: Vec<CalleeGuard>,
}
impl ConversionFileFacts {
    #[tracing::instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "ConversionFileFacts")]
    pub(crate) fn collect(file: &SharedParsedFile) -> Self {
        let mut collector = Collector {
            file,
            events: Vec::new(),
            depth: 0,
            owner: None,
            error_trait: None,
            source_site: None,
            returns: Some(BTreeSet::new()),
            try_evidence: None,
            guards: Vec::new(),
        };
        collector.visit_file(file.syntax());
        Self::new(collector.events)
    }
}
impl Collector<'_> {
    #[tracing::instrument(level = "trace", skip(self, span, kind))]
    fn record(&mut self, span: proc_macro2::Span, kind: Kind) {
        self.events.push(Event::new(
            (span.start().line, span.start().column),
            (span.end().line, span.end().column),
            self.depth,
            self.owner.clone(),
            self.guards.clone(),
            kind,
        ));
    }
    #[tracing::instrument(level = "trace", skip(self, output, scope))]
    fn enter(
        &mut self,
        output: Option<&syn::ReturnType>,
        scope: ErrorTryScope,
    ) -> (Option<BTreeSet<String>>, Option<ErrorTryEvidence>) {
        let names = output.map(|output| match output {
            syn::ReturnType::Type(_, ty) => mentions(ty),
            _ => BTreeSet::new(),
        });
        (
            std::mem::replace(&mut self.returns, names),
            self.try_evidence
                .replace(ErrorTryEvidence::for_scope(output, scope)),
        )
    }
    #[tracing::instrument(level = "trace", skip(self, previous))]
    fn leave(&mut self, previous: (Option<BTreeSet<String>>, Option<ErrorTryEvidence>)) {
        (self.returns, self.try_evidence) = previous;
    }
}
impl<'ast> Visit<'ast> for Collector<'_> {
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.depth += 1;
        syn::visit::visit_item_mod(self, item);
        self.depth -= 1;
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let previous = std::mem::replace(&mut self.owner, simple_type_name(&item.self_ty));
        let previous_trait = std::mem::replace(
            &mut self.error_trait,
            item.trait_
                .as_ref()
                .map(|(path, _)| path)
                .filter(|path| trait_is_std_error(path))
                .cloned(),
        );
        if let Some(evidence) = ErrorFromEvidence::from_impl(item) {
            let mut names = Names::default();
            if let Some((path, _)) = &item.trait_ {
                names.visit_path(path);
            }
            self.record(
                item.span(),
                Kind::From {
                    mentions: names.0,
                    evidence,
                },
            );
        }
        syn::visit::visit_item_impl(self, item);
        self.owner = previous;
        self.error_trait = previous_trait;
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let previous_source = self.source_site.take();
        if item.sig.ident == "source"
            && self.owner.is_some()
            && let Some(path) = &self.error_trait
        {
            let evidence = ErrorSourceMethodEvidence::from_method(self.file, item, path);
            self.source_site = Some(self.events.len());
            self.record(item.span(), Kind::Source(evidence));
        }
        let previous = self.enter(Some(&item.sig.output), ErrorTryScope::Function);
        syn::visit::visit_impl_item_fn(self, item);
        self.leave(previous);
        self.source_site = previous_source;
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = self.enter(Some(&item.sig.output), ErrorTryScope::Function);
        syn::visit::visit_item_fn(self, item);
        self.leave(previous);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_expr_closure(&mut self, item: &'ast syn::ExprClosure) {
        let (output, scope) = match &item.output {
            syn::ReturnType::Default => (None, ErrorTryScope::InferredClosure),
            output => (Some(output), ErrorTryScope::Closure),
        };
        let previous = self.enter(output, scope);
        syn::visit::visit_expr_closure(self, item);
        self.leave(previous);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_expr_async(&mut self, item: &'ast syn::ExprAsync) {
        let previous = self.enter(None, ErrorTryScope::AsyncBlock);
        syn::visit::visit_expr_async(self, item);
        self.leave(previous);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_expr_try_block(&mut self, item: &'ast syn::ExprTryBlock) {
        let previous = self.enter(None, ErrorTryScope::TryBlock);
        syn::visit::visit_expr_try_block(self, item);
        self.leave(previous);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_expr_struct(&mut self, item: &'ast syn::ExprStruct) {
        self.record(
            item.span(),
            Kind::Record {
                path: path(&item.path, item.qself.is_some()),
                rest: item.rest.is_some(),
            },
        );
        syn::visit::visit_expr_struct(self, item);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_expr_call(&mut self, item: &'ast syn::ExprCall) {
        if let syn::Expr::Path(callee) = &*item.func {
            let path = path(&callee.path, callee.qself.is_some());
            self.record(item.span(), Kind::Caller(path.clone()));
            self.guards.push(CalleeGuard::new(path, self.owner.clone()));
            self.visit_expr(&item.func);
            self.guards.pop();
            for arg in &item.args {
                self.visit_expr(arg);
            }
        } else {
            syn::visit::visit_expr_call(self, item);
        }
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_expr_path(&mut self, item: &'ast syn::ExprPath) {
        self.record(
            item.span(),
            Kind::Associated(path(&item.path, item.qself.is_some())),
        );
        syn::visit::visit_expr_path(self, item);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_expr_field(&mut self, item: &'ast syn::ExprField) {
        if let syn::Member::Named(name) = &item.member {
            if matches!(&*item.base, syn::Expr::Path(path) if path.qself.is_none() && path.path.is_ident("self"))
                && let Some(index) = self.source_site
                && let Kind::Source(evidence) = &mut self.events[index].kind
            {
                evidence.record_self_field(name);
            }
            self.record(item.span(), Kind::Field(name.to_string()));
        }
        syn::visit::visit_expr_field(self, item);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_pat_struct(&mut self, item: &'ast syn::PatStruct) {
        self.record(
            item.span(),
            Kind::Pattern {
                path: path(&item.path, item.qself.is_some()),
                fields: item
                    .fields
                    .iter()
                    .filter_map(|field| match &field.member {
                        syn::Member::Named(name) => Some(name.to_string()),
                        _ => None,
                    })
                    .collect(),
            },
        );
        syn::visit::visit_pat_struct(self, item);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_expr_try(&mut self, item: &'ast syn::ExprTry) {
        self.record(
            item.span(),
            Kind::Try {
                returns: self.returns.clone(),
                evidence: self.try_evidence.clone(),
            },
        );
        syn::visit::visit_expr_try(self, item);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_type_path(&mut self, item: &'ast syn::TypePath) {
        self.record(
            item.span(),
            Kind::Type(path(&item.path, item.qself.is_some())),
        );
        syn::visit::visit_type_path(self, item);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        let mut names = mentions(&item.ty);
        names.insert(item.ident.to_string());
        let stable_error = names
            .iter()
            .find(|name| stable_result_alias(item, self.file.syntax(), name))
            .cloned();
        self.record(
            item.span(),
            Kind::Alias {
                names,
                stable_error,
            },
        );
        syn::visit::visit_item_type(self, item);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let (mut names, mut glob) = (BTreeSet::new(), false);
        imports(&item.tree, &mut names, &mut glob);
        self.record(item.span(), Kind::Import { names, glob });
        syn::visit::visit_item_use(self, item);
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        let mut names = BTreeSet::new();
        tokens(item.tokens.clone(), &mut names);
        let mut constructs = BTreeSet::new();
        constructions(item.tokens.clone(), &mut constructs);
        let fields: BTreeSet<_> = names
            .iter()
            .filter(|name| tokens_access_field(&item.tokens, name))
            .cloned()
            .collect();
        let unhandled = fields
            .iter()
            .filter(|name| macro_has_unhandled_field_use(self.file, item, name))
            .cloned()
            .collect();
        self.record(
            item.span(),
            Kind::Macro {
                constructs,
                fields,
                unhandled,
            },
        );
    }
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        for attr in &item.attrs {
            let blocking = if attr.path().is_ident("derive") {
                !derive_is_nonconstructing(attr)
            } else {
                !matches!(
                    attr.path().get_ident().map(ToString::to_string).as_deref(),
                    Some("allow" | "warn" | "deny" | "forbid" | "cfg" | "doc" | "repr")
                )
            };
            if blocking {
                self.record(
                    attr.span(),
                    Kind::Attribute {
                        name: item.ident.to_string(),
                        stable_file: call_site_attribute_is_stable(
                            attr,
                            &item.attrs,
                            "file",
                            self.file.syntax(),
                        ),
                    },
                );
            }
        }
        for field in &item.fields {
            let edge_field =
                field
                    .ident
                    .as_ref()
                    .and_then(|name| match name.to_string().as_str() {
                        "source" => Some(ErrorEdgeField::Source),
                        "kind" => Some(ErrorEdgeField::Kind),
                        _ => None,
                    });
            if let Some(edge_field) = edge_field {
                self.record(
                    field.span(),
                    Kind::Edge {
                        name: item.ident.to_string(),
                        evidence: error_edge_evidence(self.file, field, edge_field),
                    },
                );
            }
        }
        syn::visit::visit_item_struct(self, item);
    }
}
#[tracing::instrument(level = "trace", skip(path))]
fn path(path: &syn::Path, qualified: bool) -> PathFact {
    PathFact::new(
        path.segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect(),
        path.get_ident().is_some(),
        qualified,
    )
}
#[derive(Default)]
struct Names(BTreeSet<String>);
impl<'ast> Visit<'ast> for Names {
    #[tracing::instrument(level = "trace", skip(self, path))]
    fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
        self.0
            .extend(path.path.segments.iter().map(|part| part.ident.to_string()));
        syn::visit::visit_type_path(self, path);
    }
}
#[tracing::instrument(level = "trace", skip(ty))]
fn mentions(ty: &syn::Type) -> BTreeSet<String> {
    let mut names = Names::default();
    names.visit_type(ty);
    names.0
}
#[tracing::instrument(level = "trace", skip(tree, names, glob))]
fn imports(tree: &syn::UseTree, names: &mut BTreeSet<String>, glob: &mut bool) {
    match tree {
        syn::UseTree::Path(part) => {
            names.insert(part.ident.to_string());
            imports(&part.tree, names, glob);
        }
        syn::UseTree::Name(part) => {
            names.insert(part.ident.to_string());
        }
        syn::UseTree::Rename(part) => {
            names.insert(part.ident.to_string());
            names.insert(part.rename.to_string());
        }
        syn::UseTree::Group(group) => {
            for part in &group.items {
                imports(part, names, glob);
            }
        }
        syn::UseTree::Glob(_) => *glob = true,
    }
}
#[tracing::instrument(level = "trace", skip(stream, names))]
fn tokens(stream: proc_macro2::TokenStream, names: &mut BTreeSet<String>) {
    for token in stream {
        match token {
            proc_macro2::TokenTree::Ident(name) => {
                names.insert(name.to_string());
            }
            proc_macro2::TokenTree::Group(group) => tokens(group.stream(), names),
            _ => {}
        }
    }
}

#[tracing::instrument(level = "trace", skip(stream, names))]
fn constructions(stream: proc_macro2::TokenStream, names: &mut BTreeSet<String>) {
    let parts: Vec<_> = stream.into_iter().collect();
    for (index, part) in parts.iter().enumerate() {
        if let proc_macro2::TokenTree::Ident(name) = part
            && matches!(parts.get(index + 1), Some(proc_macro2::TokenTree::Group(group)) if group.delimiter() == proc_macro2::Delimiter::Brace)
        {
            names.insert(name.to_string());
        }
        if let proc_macro2::TokenTree::Group(group) = part {
            constructions(group.stream(), names);
        }
    }
}
