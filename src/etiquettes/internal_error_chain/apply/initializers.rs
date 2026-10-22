//! Constructor initializer recognition for conservative error field edits.

use std::collections::{BTreeMap, BTreeSet};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprStruct, FnArg, ImplItemFn, ItemFn, ItemImpl, Member, Pat, Signature};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::{Edit, is_static_str, simple_type_name, span_range};

pub(super) struct InitializerVisitor<'a> {
    type_name: &'a str,
    field_name: &'a str,
    kind: FieldKind,
    strategy: ErrorFieldStrategy,
    source: &'a str,
    static_strings: &'a BTreeSet<String>,
    current_impl: Option<String>,
    caller_aliases: BTreeSet<String>,
    static_params: BTreeSet<String>,
    edits: Vec<Edit>,
    found: usize,
    unresolved: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum FieldKind {
    CallSiteFile,
    StaticString,
}

impl<'a> InitializerVisitor<'a> {
    #[instrument(level = "debug", skip(kind, strategy, source, static_strings))]
    pub(super) fn new(
        type_name: &'a str,
        field_name: &'a str,
        kind: FieldKind,
        strategy: ErrorFieldStrategy,
        source: &'a str,
        static_strings: &'a BTreeSet<String>,
    ) -> Self {
        Self {
            type_name,
            field_name,
            kind,
            strategy,
            source,
            static_strings,
            current_impl: None,
            caller_aliases: BTreeSet::new(),
            static_params: BTreeSet::new(),
            edits: Vec::new(),
            found: 0,
            unresolved: false,
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn finish(self) -> Option<Vec<Edit>> {
        (self.found > 0 && !self.unresolved).then_some(self.edits)
    }

    #[instrument(level = "debug", skip(self, signature, block))]
    fn visit_constructor_block(&mut self, signature: &Signature, block: &syn::Block) {
        let previous = std::mem::replace(&mut self.caller_aliases, caller_aliases(block));
        let previous_params = std::mem::replace(
            &mut self.static_params,
            static_string_params(signature, block),
        );
        syn::visit::visit_block(self, block);
        self.caller_aliases = previous;
        self.static_params = previous_params;
    }
}

impl<'ast> Visit<'ast> for InitializerVisitor<'_> {
    #[instrument(level = "debug", skip(self, _item))]
    fn visit_item_mod(&mut self, _item: &'ast syn::ItemMod) {
        // A nested module has its own `Self` and privacy context.
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let previous = std::mem::replace(&mut self.current_impl, simple_type_name(&item.self_ty));
        syn::visit::visit_item_impl(self, item);
        self.current_impl = previous;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.visit_constructor_block(&item.sig, &item.block);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.visit_constructor_block(&item.sig, &item.block);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_expr_struct(&mut self, item: &'ast ExprStruct) {
        let target = item.path.is_ident(self.type_name)
            || (item.path.is_ident("Self") && self.current_impl.as_deref() == Some(self.type_name));
        if !target
            && item
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == self.type_name)
        {
            self.unresolved = true;
        }
        if target {
            self.found += 1;
            if item.rest.is_some() {
                self.unresolved = true;
            } else if let Some(field) = item.fields.iter().find(
                |field| matches!(&field.member, Member::Named(name) if name == self.field_name),
            ) {
                let replacement = match self.kind {
                    FieldKind::CallSiteFile => file_initializer_replacement(
                        self.source,
                        &field.expr,
                        self.strategy,
                        &self.caller_aliases,
                    ),
                    FieldKind::StaticString => static_string_initializer_replacement(
                        self.source,
                        &field.expr,
                        self.strategy,
                        self.static_strings,
                        &self.static_params,
                    ),
                };
                if let (Some((start, end)), Some(replacement)) =
                    (span_range(self.source, field.expr.span()), replacement)
                {
                    self.edits.push(Edit {
                        start,
                        end,
                        replacement,
                    });
                } else {
                    self.unresolved = true;
                }
            } else {
                self.unresolved = true;
            }
        }
        syn::visit::visit_expr_struct(self, item);
    }
}

#[instrument(level = "debug", skip(source, expr, strategy, aliases))]
fn file_initializer_replacement(
    source: &str,
    expr: &Expr,
    strategy: ErrorFieldStrategy,
    aliases: &BTreeSet<String>,
) -> Option<String> {
    let expr = without_parens(expr);
    let file_call = match strategy {
        ErrorFieldStrategy::Borrowed => match expr {
            Expr::MethodCall(method)
                if matches!(
                    method.method.to_string().as_str(),
                    "to_string" | "to_owned" | "into"
                ) && method.args.is_empty() =>
            {
                &*method.receiver
            }
            _ => string_from_argument(expr)?,
        },
        ErrorFieldStrategy::Owned => expr,
    };
    let file_call = without_parens(file_call);
    let Expr::MethodCall(file_method) = file_call else {
        return None;
    };
    if file_method.method != "file" || !file_method.args.is_empty() {
        return None;
    }
    let valid_receiver = match without_parens(&file_method.receiver) {
        Expr::Path(path) => path
            .path
            .get_ident()
            .is_some_and(|name| aliases.contains(&name.to_string())),
        Expr::Call(call) => is_location_caller(call),
        _ => false,
    };
    if !valid_receiver {
        return None;
    }
    let (start, end) = span_range(source, file_call.span())?;
    let file_text = source.get(start..end)?;
    Some(match strategy {
        ErrorFieldStrategy::Borrowed => file_text.to_string(),
        ErrorFieldStrategy::Owned => format!("{file_text}.to_string()"),
    })
}

#[instrument(
    level = "debug",
    skip(source, expr, strategy, static_strings, static_params)
)]
fn static_string_initializer_replacement(
    source: &str,
    expr: &Expr,
    strategy: ErrorFieldStrategy,
    static_strings: &BTreeSet<String>,
    static_params: &BTreeSet<String>,
) -> Option<String> {
    let expr = without_parens(expr);
    let value = match strategy {
        ErrorFieldStrategy::Borrowed => match expr {
            Expr::MethodCall(method)
                if matches!(
                    method.method.to_string().as_str(),
                    "to_string" | "to_owned" | "into"
                ) && method.args.is_empty() =>
            {
                &*method.receiver
            }
            Expr::Call(_) => string_from_argument(expr)?,
            _ => return None,
        },
        ErrorFieldStrategy::Owned => expr,
    };
    let value = without_parens(value);
    let parameter = matches!(value, Expr::Path(path)
        if path.qself.is_none()
            && path.path.get_ident().is_some_and(|name| static_params.contains(&name.to_string())));
    if parameter && strategy == ErrorFieldStrategy::Borrowed && !matches!(expr, Expr::Call(_)) {
        return None;
    }
    let literal = matches!(value, Expr::Lit(lit) if matches!(&lit.lit, syn::Lit::Str(_)));
    let qualified_const = matches!(value, Expr::Path(path)
        if path.qself.is_none()
            && matches!(path.path.segments.iter().map(|part| part.ident.to_string()).collect::<Vec<_>>().as_slice(), [scope, name]
                if scope == "self" && static_strings.contains(name)));
    let qualified_builtin_macro = matches!(value, Expr::Macro(macro_expr)
        if macro_expr.mac.path.leading_colon.is_some()
            && matches!(macro_expr.mac.path.segments.iter().map(|part| part.ident.to_string()).collect::<Vec<_>>().as_slice(), [root, name]
                if (root == "core" || root == "std") && (name == "concat" || name == "include_str")));
    if !literal && !qualified_const && !qualified_builtin_macro && !parameter {
        return None;
    }
    let (start, end) = span_range(source, value.span())?;
    let value_text = source.get(start..end)?;
    Some(match strategy {
        ErrorFieldStrategy::Borrowed => value_text.to_string(),
        ErrorFieldStrategy::Owned if parameter => format!("String::from({value_text})"),
        ErrorFieldStrategy::Owned => format!("{value_text}.to_string()"),
    })
}

#[instrument(level = "debug", skip(signature, block))]
fn static_string_params(signature: &Signature, block: &syn::Block) -> BTreeSet<String> {
    let mut params: BTreeSet<String> = signature
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(arg) if is_static_str(&arg.ty) => match &*arg.pat {
                Pat::Ident(ident) if ident.subpat.is_none() => Some(ident.ident.to_string()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let mut shadows = BindingNames::default();
    shadows.visit_block(block);
    params.retain(|name| !shadows.0.contains(name));
    params
}

#[derive(Default)]
struct BindingNames(BTreeSet<String>);

impl<'ast> Visit<'ast> for BindingNames {
    #[instrument(level = "debug", skip(self, ident))]
    fn visit_pat_ident(&mut self, ident: &'ast syn::PatIdent) {
        self.0.insert(ident.ident.to_string());
        syn::visit::visit_pat_ident(self, ident);
    }
}

#[instrument(level = "debug", skip(expr))]
fn string_from_argument(expr: &Expr) -> Option<&Expr> {
    let Expr::Call(call) = without_parens(expr) else {
        return None;
    };
    if call.args.len() != 1 {
        return None;
    }
    let Expr::Path(path) = without_parens(&call.func) else {
        return None;
    };
    if path.qself.is_some() {
        return None;
    }
    let names: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    if !matches!(names.as_slice(), [string, from] if string == "String" && from == "from")
        && !matches!(names.as_slice(), [root, module, string, from]
            if (root == "std" || root == "alloc") && module == "string" && string == "String" && from == "from")
    {
        return None;
    }
    call.args.first()
}

#[instrument(level = "debug", skip(expr))]
fn without_parens(mut expr: &Expr) -> &Expr {
    while let Expr::Paren(paren) = expr {
        expr = &paren.expr;
    }
    expr
}

#[instrument(level = "debug", skip(block))]
fn caller_aliases(block: &syn::Block) -> BTreeSet<String> {
    let mut aliases: BTreeSet<String> = block.stmts.iter().filter_map(|stmt| {
        let syn::Stmt::Local(local) = stmt else { return None };
        let Pat::Ident(pat) = &local.pat else { return None };
        if pat.mutability.is_some() || !matches!(local.init.as_ref().map(|init| &*init.expr), Some(Expr::Call(call)) if is_location_caller(call)) {
            return None;
        }
        Some(pat.ident.to_string())
    }).collect();
    let mut bindings = BindingCounts::default();
    bindings.visit_block(block);
    aliases.retain(|name| bindings.0.get(name) == Some(&1));
    aliases
}

#[derive(Default)]
struct BindingCounts(BTreeMap<String, usize>);

impl<'ast> Visit<'ast> for BindingCounts {
    #[instrument(level = "debug", skip(self, pat))]
    fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
        *self.0.entry(pat.ident.to_string()).or_default() += 1;
        syn::visit::visit_pat_ident(self, pat);
    }
}

#[instrument(level = "trace", skip(call))]
fn is_location_caller(call: &syn::ExprCall) -> bool {
    if !call.args.is_empty() {
        return false;
    }
    let Expr::Path(path) = &*call.func else {
        return false;
    };
    let names: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    matches!(names.as_slice(), [location, caller] if location == "Location" && caller == "caller")
        || matches!(names.as_slice(), [root, panic, location, caller]
            if (root == "std" || root == "core") && panic == "panic" && location == "Location" && caller == "caller")
}
