//! Strategy-independent expression sites collected from one shared source tree.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use syn::Expr;
use syn::spanned::Spanned;
use syn::visit::Visit;
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::super::super::span_range;
use super::super::super::uses::macro_has_unhandled_field_use;
use super::super::lifetimes::{anchored_parameters, constructor_origin};
use super::super::types::type_name;

type Range = (usize, usize);

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(in crate::etiquettes::internal_error_chain::apply) struct GraphExprFacts {
    events: Vec<GraphExprEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum GraphExprEvent {
    Struct {
        name: String,
        has_rest: bool,
        fields: Vec<(String, GraphValueFact)>,
    },
    Variant {
        owner: String,
        variant: String,
        args: Vec<GraphValueFact>,
    },
    SourceMethod {
        owner: String,
        argument: SourceArgument,
    },
    KindAsRef {
        owner: String,
        receiver: Option<Range>,
        whole: Option<Range>,
    },
    KindMatch {
        owner: String,
        subject: Option<Range>,
    },
    FieldUse {
        owner: String,
        field: String,
        self_base: bool,
        range: Option<Range>,
    },
    Pattern {
        name: String,
        fields: BTreeSet<String>,
    },
    Macro {
        constructed_types: BTreeSet<String>,
        unhandled_fields: BTreeSet<String>,
        mentioned_types: BTreeSet<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum SourceArgument {
    BorrowedReference {
        field: Option<Range>,
        reference: Option<Range>,
    },
    OwnedField {
        field: Option<Range>,
    },
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(super) struct GraphValueFact {
    #[getter(copy)]
    range: Option<Range>,
    #[getter(copy)]
    borrowed_string: Option<Range>,
    #[getter(copy)]
    borrowed_value: Option<Range>,
    #[getter(copy)]
    borrowed_boxed: Option<Range>,
    anchored_lifetimes: BTreeSet<String>,
}

impl GraphExprFacts {
    #[instrument(level = "debug", skip(file, tracked_names, tracked_fields))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn collect(
        file: &SharedParsedFile,
        tracked_names: &BTreeSet<String>,
        tracked_fields: &BTreeSet<String>,
    ) -> Self {
        let mut collector = Collector {
            file,
            tracked_names,
            tracked_fields,
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
    tracked_names: &'a BTreeSet<String>,
    tracked_fields: &'a BTreeSet<String>,
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
                .filter_map(|(lifetime, params)| {
                    constructor_origin(expr, params).then(|| lifetime.clone())
                })
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
        if let Some(name) = name.filter(|name| self.tracked_names.contains(name)) {
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
            if let [.., owner, variant] = names.as_slice()
                && self.tracked_names.contains(owner)
            {
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
        if let syn::Member::Named(name) = &item.member
            && self.tracked_fields.contains(&name.to_string())
        {
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
        if let Some(name) = name.filter(|name| self.tracked_names.contains(name)) {
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
        let mentioned_types: BTreeSet<_> = self
            .tracked_names
            .intersection(&tokens.identifiers)
            .cloned()
            .collect();
        let constructed_types: BTreeSet<_> = if tokens.constructed.contains("Self") {
            self.tracked_names.clone()
        } else {
            self.tracked_names
                .intersection(&tokens.constructed)
                .cloned()
                .collect()
        };
        let possible_fields: BTreeSet<_> = if tokens.constructed.contains("Self") {
            self.tracked_fields.clone()
        } else {
            self.tracked_fields
                .intersection(&tokens.accessed_fields)
                .cloned()
                .collect()
        };
        let unhandled_fields: BTreeSet<_> = possible_fields
            .into_iter()
            .filter(|field| macro_has_unhandled_field_use(item, field))
            .collect();
        if !constructed_types.is_empty()
            || !unhandled_fields.is_empty()
            || !mentioned_types.is_empty()
        {
            self.facts.events.push(GraphExprEvent::Macro {
                constructed_types,
                unhandled_fields,
                mentioned_types,
            });
        }
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
