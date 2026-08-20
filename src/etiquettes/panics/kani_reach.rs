//! Crate-local reachability from `#[kani::proof]` harnesses.
//!
//! Kani checks reachable panics/aborts, not a harness's return value --
//! confirmed against a real `cargo kani` run: a `#[kani::proof]` fn
//! returning `Result<(), E>` that returns `Err(..)` with no panic
//! verifies as `SUCCESSFUL`. So a `panic!`/`unreachable!`/`.expect()`/
//! `.unwrap()` that only ever executes as part of a proof harness's own
//! symbolic exploration *is* that harness's failure mechanism, not a
//! library surface that should return a typed error instead -- routing
//! it through `Result` would silently disable the check.
//!
//! This computes, per crate, the set of function/method names reachable
//! from a `#[kani::proof]` harness (or from anywhere nested inside a
//! `#[cfg(kani)]` item, since that code exists only during Kani
//! verification in the first place) by a fixed-point closure over a
//! name-based call graph built from the same parsed source the panics
//! scan already walks.
//!
//! Deliberately coarse, in one specific way: every function and method
//! is keyed by its bare identifier, not `Type::method` -- a method call
//! site (`receiver.demonstrate_delivery(..)`) can't be resolved to its
//! defining type without real type inference, so edges from a method
//! call have to target the same bare-name key a method definition is
//! recorded under, or reachability could never connect the two. This
//! can over-match same-named functions in unrelated types; it can never
//! under-match (miss a real reachable site), which is the safe
//! direction for an exemption -- the failure mode of over-matching is
//! a real library panic staying unflagged a little longer, not a real
//! proof-failure mechanism getting flagged as if it were a bug.
//!
//! Scoped to item-level `#[cfg(kani)]`/`#[kani::proof]`; an
//! expression-level `#[cfg(kani)] { .. }` block (as opposed to
//! `#[cfg(not(kani))]`, which this deliberately does NOT treat as
//! reachable) is not tracked -- no motivating case needed it.

use std::collections::{HashMap, HashSet};

use proc_macro2::{Delimiter, TokenTree};
use syn::visit::Visit;
use syn::{
    Expr, ExprCall, ExprMethodCall, File, ImplItemFn, Item, ItemFn, ItemImpl, ItemMacro, ItemMod,
};

use tracing::instrument;

use super::scan::has_cfg_flag;

/// Best-effort recovery of real items wrapped inside an opaque macro
/// invocation -- `syn::visit::Visit` never descends into a macro's own
/// token stream, so a `#[kani::proof] fn ..` written as the argument to
/// a wrapper macro (this codebase's own `amenable_derive::harness! {
/// cfg_name, CONST_NAME, { item } }`, which exists specifically to
/// capture the item's verbatim source into a sibling constant) is
/// otherwise invisible to root/call-graph detection entirely. Tries the
/// whole token stream as a sequence of items first (covers a
/// `macro_rules!`-style body that's items outright), then falls back to
/// the last brace-delimited group in the token stream (covers `macro!(
/// args.., { item })`, `harness!`'s own shape) parsed as one item.
/// Silently yields nothing if neither parses -- this is a best-effort
/// widening of what counts as reachable, not a requirement.
#[instrument(level = "trace", skip(tokens))]
fn items_inside_macro(tokens: proc_macro2::TokenStream) -> Vec<Item> {
    if let Ok(file) = syn::parse2::<File>(tokens.clone()) {
        return file.items;
    }

    let last_brace_group = tokens
        .into_iter()
        .filter_map(|tree| match tree {
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => Some(group),
            _ => None,
        })
        .last();
    let Some(group) = last_brace_group else {
        return Vec::new();
    };

    syn::parse2::<Item>(group.stream())
        .ok()
        .into_iter()
        .collect()
}

/// Crate-local function/method name reachable from a `#[kani::proof]`
/// harness (or `#[cfg(kani)]`-only code), by fixed-point call-graph
/// closure. Bare identifiers, not qualified paths -- see module doc.
#[derive(Debug, Default)]
pub(super) struct KaniReachability {
    reachable: HashSet<String>,
}

impl KaniReachability {
    #[instrument(level = "trace", skip(self, key), ret)]
    pub(super) fn contains(&self, key: &str) -> bool {
        self.reachable.contains(key)
    }
}

/// Build the reachability set from a crate's already-parsed source files.
#[instrument(level = "debug", skip(files))]
pub(super) fn build_kani_reachability<'a>(
    files: impl Iterator<Item = &'a File>,
) -> KaniReachability {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    let mut roots: HashSet<String> = HashSet::new();

    for syntax in files {
        let mut visitor = GraphVisitor {
            fn_stack: Vec::new(),
            in_cfg_kani: false,
            graph: &mut graph,
            roots: &mut roots,
        };
        visitor.visit_file(syntax);
    }

    let mut reachable: HashSet<String> = roots.clone();
    let mut frontier: Vec<String> = roots.into_iter().collect();
    while let Some(key) = frontier.pop() {
        let Some(calls) = graph.get(&key) else {
            continue;
        };
        for call in calls {
            if reachable.insert(call.clone()) {
                frontier.push(call.clone());
            }
        }
    }

    KaniReachability { reachable }
}

struct GraphVisitor<'a> {
    fn_stack: Vec<String>,
    in_cfg_kani: bool,
    graph: &'a mut HashMap<String, Vec<String>>,
    roots: &'a mut HashSet<String>,
}

impl GraphVisitor<'_> {
    #[instrument(level = "trace", skip(attrs))]
    fn is_kani_proof(attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|attr| {
            let segments = &attr.path().segments;
            segments.len() == 2 && segments[0].ident == "kani" && segments[1].ident == "proof"
        })
    }

    #[instrument(level = "trace", skip(self, name))]
    fn enter_fn(&mut self, name: String, is_root: bool) {
        if is_root {
            self.roots.insert(name.clone());
        }
        self.graph.entry(name.clone()).or_default();
        self.fn_stack.push(name);
    }

    #[instrument(level = "trace", skip(self))]
    fn exit_fn(&mut self) {
        self.fn_stack.pop();
    }

    #[instrument(level = "trace", skip(self, callee))]
    fn record_call(&mut self, callee: String) {
        let Some(caller) = self.fn_stack.last() else {
            return;
        };
        self.graph.entry(caller.clone()).or_default().push(callee);
    }
}

impl<'ast> Visit<'ast> for GraphVisitor<'_> {
    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        let prev = self.in_cfg_kani;
        if has_cfg_flag(&node.attrs, "kani") {
            self.in_cfg_kani = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_cfg_kani = prev;
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let prev = self.in_cfg_kani;
        if has_cfg_flag(&node.attrs, "kani") {
            self.in_cfg_kani = true;
        }
        syn::visit::visit_item_impl(self, node);
        self.in_cfg_kani = prev;
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let prev = self.in_cfg_kani;
        let is_kani_only = self.in_cfg_kani || has_cfg_flag(&node.attrs, "kani");
        if is_kani_only {
            self.in_cfg_kani = true;
        }
        let is_root = is_kani_only || Self::is_kani_proof(&node.attrs);
        self.enter_fn(node.sig.ident.to_string(), is_root);
        syn::visit::visit_item_fn(self, node);
        self.exit_fn();
        self.in_cfg_kani = prev;
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        let prev = self.in_cfg_kani;
        let is_kani_only = self.in_cfg_kani || has_cfg_flag(&node.attrs, "kani");
        if is_kani_only {
            self.in_cfg_kani = true;
        }
        let is_root = is_kani_only || Self::is_kani_proof(&node.attrs);
        self.enter_fn(node.sig.ident.to_string(), is_root);
        syn::visit::visit_impl_item_fn(self, node);
        self.exit_fn();
        self.in_cfg_kani = prev;
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Expr::Path(path) = node.func.as_ref()
            && let Some(segment) = path.path.segments.last()
        {
            self.record_call(segment.ident.to_string());
        }
        syn::visit::visit_expr_call(self, node);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        self.record_call(node.method.to_string());
        syn::visit::visit_expr_method_call(self, node);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_macro(&mut self, node: &'ast ItemMacro) {
        for item in items_inside_macro(node.mac.tokens.clone()) {
            syn::visit::visit_item(self, &item);
        }
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        for item in items_inside_macro(node.mac.tokens.clone()) {
            syn::visit::visit_item(self, &item);
        }
        syn::visit::visit_stmt_macro(self, node);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
        for item in items_inside_macro(node.mac.tokens.clone()) {
            syn::visit::visit_item(self, &item);
        }
        syn::visit::visit_expr_macro(self, node);
    }
}
