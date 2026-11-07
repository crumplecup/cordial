//! Compatibility collection at the loader's shared-tree boundary.
use super::super::super::span_range;
use super::super::super::uses::macro_has_unhandled_field_use;
use super::super::lifetimes::{anchored_parameters, constructor_origin};
use super::super::types::type_name;
use super::{GraphExprEvent, GraphExprFacts, GraphValueFact, Range, SourceArgument};
use crate::loader::SharedParsedFile;
use std::collections::{BTreeMap, BTreeSet};
use syn::{Expr, spanned::Spanned, visit::Visit};
use tracing::instrument;

impl GraphExprFacts {
    #[instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "GraphExprFacts")]
    pub(crate) fn collect(file: &SharedParsedFile) -> Self {
        let mut collector = Collector {
            file,
            facts: Self { events: Vec::new() },
            current_impl: None,
            current_error_trait: false,
            anchor_params: BTreeMap::new(),
        };
        collector.visit_file(file.syntax());
        collector.facts
    }
}

struct Collector<'a> {
    file: &'a SharedParsedFile,
    facts: GraphExprFacts,
    current_impl: Option<String>,
    current_error_trait: bool,
    anchor_params: BTreeMap<String, BTreeSet<String>>,
}

impl Collector<'_> {
    #[instrument(level = "trace", skip(self, span))]
    fn range(&self, span: proc_macro2::Span) -> Option<Range> {
        span_range(self.file.source(), span)
    }

    #[instrument(level = "debug", skip(self, expr))]
    fn value(&self, expr: &Expr) -> GraphValueFact {
        let method_receiver = match expr {
            Expr::MethodCall(call) if call.args.is_empty() => Some(call),
            _ => None,
        };
        let borrowed_string = method_receiver
            .filter(|call| {
                matches!(
                    call.method.to_string().as_str(),
                    "clone" | "to_owned" | "to_string"
                )
            })
            .and_then(|call| self.range(call.receiver.span()));
        let borrowed_value = method_receiver
            .filter(|call| call.method == "clone")
            .and_then(|call| self.range(call.receiver.span()));
        let borrowed_boxed = match expr {
            Expr::Call(call)
                if call.args.len() == 1
                    && matches!(&*call.func, Expr::Path(path)
                        if path.path.segments.first().is_some_and(|segment| segment.ident == "Box")
                            && path.path.segments.last().is_some_and(|segment| segment.ident == "new")) =>
            {
                match call.args.first() {
                    Some(Expr::MethodCall(inner))
                        if inner.args.is_empty() && inner.method == "clone" =>
                    {
                        self.range(inner.receiver.span())
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        GraphValueFact {
            range: self.range(expr.span()),
            borrowed_string,
            borrowed_value,
            borrowed_boxed,
            anchored_lifetimes: self
                .anchor_params
                .iter()
                .filter(|&(_lifetime, params)| constructor_origin(expr, params))
                .map(|(lifetime, _params)| lifetime.clone())
                .collect(),
        }
    }

    #[instrument(level = "debug", skip(signature))]
    fn parameters(signature: &syn::Signature) -> BTreeMap<String, BTreeSet<String>> {
        struct Lifetimes(BTreeSet<String>);
        impl<'ast> Visit<'ast> for Lifetimes {
            fn visit_lifetime(&mut self, lifetime: &'ast syn::Lifetime) {
                self.0.insert(lifetime.ident.to_string());
            }
        }
        let mut lifetimes = Lifetimes(BTreeSet::new());
        lifetimes.visit_signature(signature);
        lifetimes
            .0
            .into_iter()
            .map(|lifetime| {
                let params = anchored_parameters(signature, &lifetime);
                (lifetime, params)
            })
            .collect()
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let previous = self
            .current_impl
            .replace(type_name(&item.self_ty).unwrap_or_default());
        let previous_error_trait = self.current_error_trait;
        self.current_error_trait = item.trait_.as_ref().is_some_and(|(path, _)| {
            let names: Vec<_> = path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            names == ["std", "error", "Error"] || names == ["core", "error", "Error"]
        });
        syn::visit::visit_item_impl(self, item);
        self.current_impl = previous;
        self.current_error_trait = previous_error_trait;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let previous = std::mem::replace(&mut self.anchor_params, Self::parameters(&item.sig));
        if self.current_error_trait && item.sig.ident == "source" {
            let argument = item.block.stmts.last().and_then(|stmt| match stmt {
                syn::Stmt::Expr(Expr::Call(call), None)
                    if matches!(&*call.func, Expr::Path(path) if path.path.is_ident("Some"))
                        && call.args.len() == 1 =>
                {
                    call.args.first()
                }
                _ => None,
            });
            let argument = match argument {
                Some(Expr::Reference(reference)) if self_field(&reference.expr, "source") => {
                    SourceArgument::BorrowedReference {
                        field: self.range(reference.expr.span()),
                        reference: self.range(reference.span()),
                    }
                }
                Some(expr) if self_field(expr, "source") => SourceArgument::OwnedField {
                    field: self.range(expr.span()),
                },
                _ => SourceArgument::Unsupported,
            };
            self.facts.events.push(GraphExprEvent::SourceMethod {
                owner: self.current_impl.clone().unwrap_or_default(),
                argument,
            });
        }
        syn::visit::visit_impl_item_fn(self, item);
        self.anchor_params = previous;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = std::mem::replace(&mut self.anchor_params, Self::parameters(&item.sig));
        syn::visit::visit_item_fn(self, item);
        self.anchor_params = previous;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_struct(&mut self, item: &'ast syn::ExprStruct) {
        let name = item
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string());
        let name = if name.as_deref() == Some("Self") {
            self.current_impl.clone()
        } else {
            name
        };
        if let Some(name) = name {
            let fields = item
                .fields
                .iter()
                .filter_map(|field| {
                    let syn::Member::Named(member) = &field.member else {
                        return None;
                    };
                    Some((member.to_string(), self.value(&field.expr)))
                })
                .collect();
            self.facts.events.push(GraphExprEvent::Struct {
                name,
                has_rest: item.rest.is_some(),
                fields,
            });
        }
        syn::visit::visit_expr_struct(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_call(&mut self, item: &'ast syn::ExprCall) {
        if let Expr::Path(path) = &*item.func {
            let names: Vec<_> = path
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            if let [.., owner, variant] = names.as_slice() {
                let args = item.args.iter().map(|arg| self.value(arg)).collect();
                self.facts.events.push(GraphExprEvent::Variant {
                    owner: owner.clone(),
                    variant: variant.clone(),
                    args,
                });
            }
        }
        syn::visit::visit_expr_call(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_method_call(&mut self, item: &'ast syn::ExprMethodCall) {
        if item.method == "as_ref" && item.args.is_empty() && self_field(&item.receiver, "kind") {
            self.facts.events.push(GraphExprEvent::KindAsRef {
                owner: self.current_impl.clone().unwrap_or_default(),
                receiver: self.range(item.receiver.span()),
                whole: self.range(item.span()),
            });
        }
        syn::visit::visit_expr_method_call(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_match(&mut self, item: &'ast syn::ExprMatch) {
        if self_field(&item.expr, "kind") {
            self.facts.events.push(GraphExprEvent::KindMatch {
                owner: self.current_impl.clone().unwrap_or_default(),
                subject: self.range(item.expr.span()),
            });
        }
        syn::visit::visit_expr_match(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_expr_field(&mut self, item: &'ast syn::ExprField) {
        if let syn::Member::Named(name) = &item.member {
            self.facts.events.push(GraphExprEvent::FieldUse {
                owner: self.current_impl.clone().unwrap_or_default(),
                field: name.to_string(),
                self_base: matches!(&*item.base, Expr::Path(path) if path.path.is_ident("self")),
                range: self.range(item.span()),
            });
        }
        syn::visit::visit_expr_field(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_pat_struct(&mut self, item: &'ast syn::PatStruct) {
        let name = item
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string());
        let name = if name.as_deref() == Some("Self") {
            self.current_impl.clone()
        } else {
            name
        };
        if let Some(name) = name {
            let fields = item
                .fields
                .iter()
                .filter_map(|field| match &field.member {
                    syn::Member::Named(name) => Some(name.to_string()),
                    _ => None,
                })
                .collect();
            self.facts
                .events
                .push(GraphExprEvent::Pattern { name, fields });
        }
        syn::visit::visit_pat_struct(self, item);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        let tokens = MacroTokens::collect(&item.tokens);
        self.facts.events.push(GraphExprEvent::Macro {
            constructed_types: tokens.constructed,
            unhandled_fields: tokens
                .accessed_fields
                .into_iter()
                .filter(|field| macro_has_unhandled_field_use(self.file, item, field))
                .collect(),
            mentioned_types: tokens.identifiers,
        });
        syn::visit::visit_macro(self, item);
    }
}

#[instrument(level = "debug", skip(expr))]
fn self_field(expr: &Expr, field_name: &str) -> bool {
    matches!(expr, Expr::Field(field)
        if matches!(&*field.base, Expr::Path(path) if path.path.is_ident("self"))
            && matches!(&field.member, syn::Member::Named(name) if name == field_name))
}

#[derive(Default)]
struct MacroTokens {
    identifiers: BTreeSet<String>,
    constructed: BTreeSet<String>,
    accessed_fields: BTreeSet<String>,
}

impl MacroTokens {
    #[instrument(level = "debug", skip(tokens))]
    fn collect(tokens: &proc_macro2::TokenStream) -> Self {
        let mut facts = Self::default();
        facts.visit(tokens);
        facts
    }

    #[instrument(level = "trace", skip(self, tokens))]
    fn visit(&mut self, tokens: &proc_macro2::TokenStream) {
        use proc_macro2::{Delimiter, TokenTree};

        let trees: Vec<_> = tokens.clone().into_iter().collect();
        for (index, tree) in trees.iter().enumerate() {
            match tree {
                TokenTree::Ident(ident) => {
                    let name = ident.to_string();
                    self.identifiers.insert(name.clone());
                    if matches!(trees.get(index + 1), Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Brace)
                    {
                        self.constructed.insert(name);
                    }
                }
                TokenTree::Punct(dot) if dot.as_char() == '.' => {
                    if let Some(TokenTree::Ident(name)) = trees.get(index + 1)
                        && !matches!(trees.get(index + 2), Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Parenthesis)
                    {
                        self.accessed_fields.insert(name.to_string());
                    }
                }
                TokenTree::Group(group) => self.visit(&group.stream()),
                _ => {}
            }
        }
    }
}
