//! Kani `assert!`/`assert_eq!`/`kani::assume` unnamed-bound scan.

use std::path::{Path, PathBuf};

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, Expr, ItemFn, ItemMacro, Token};
use tracing::instrument;

use crate::error::{CordialError, CordialResult};
use crate::etiquettes::antipatterns::types::AntipatternSiteRecord;
use crate::loader::module_path_from_src_file;

use super::index::{ContractIndex, is_trivial, normalize_tokens};
use super::{harness_macro_item_fn, make_finding, site_context};

#[instrument(level = "debug", skip(source, file, index), err(level = "warn"))]
pub(super) fn scan_kani_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    index: &ContractIndex,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = KaniVisitor {
        file: file.to_path_buf(),
        module_prefix,
        fn_stack: Vec::new(),
        in_kani_proof: false,
        index,
        findings: Vec::new(),
        error: None,
    };
    visitor.visit_file(&syntax);
    if let Some(err) = visitor.error {
        return Err(err);
    }
    Ok(visitor.findings)
}

struct KaniVisitor<'a> {
    file: PathBuf,
    module_prefix: Vec<String>,
    fn_stack: Vec<String>,
    in_kani_proof: bool,
    index: &'a ContractIndex,
    findings: Vec<AntipatternSiteRecord>,
    error: Option<CordialError>,
}

impl KaniVisitor<'_> {
    #[instrument(level = "trace", skip(attrs), ret)]
    fn is_kani_proof(attrs: &[Attribute]) -> bool {
        attrs.iter().any(|attr| {
            attr.path()
                .segments
                .last()
                .is_some_and(|seg| seg.ident == "proof")
                && attr.path().segments.iter().any(|seg| seg.ident == "kani")
        })
    }

    /// Common tail for every Kani clause, whichever shape it was
    /// captured from: normalize, skip trivial/registered clauses, else
    /// record a finding.
    #[instrument(level = "debug", skip(self, clause))]
    fn check_clause(&mut self, kind: &str, clause: TokenStream, line: u32) {
        let normalized = normalize_tokens(clause.clone());
        if is_trivial(&normalized) || self.index.matches_named_call("kani", kind, clause) {
            return;
        }
        let context = site_context(&self.module_prefix, &self.fn_stack.join("::"));
        self.findings
            .push(make_finding(context, &self.file, line, &normalized));
    }

    /// `assert!(EXPR, ..)`/`assert_eq!(A, B, ..)` — both ensures-shaped.
    /// `assert_eq!` synthesizes the clause `A == B` from its two
    /// comparands: a direct transcription of what's on the page, not a
    /// guess.
    ///
    /// A macro's argument tokens aren't parsed into structured `Expr`s by
    /// `syn::parse_file` (a macro's grammar is macro-specific, so `syn`
    /// only hands back the raw body), so the arguments are parsed here
    /// directly as `Punctuated<Expr, Comma>` — real Rust expression
    /// grammar, not a naive top-level-comma split. That's what correctly
    /// tells a turbofish's internal comma (`Type::<A, B>::method(..)`,
    /// `Type::<Array<i32, 3>>::method(..)`) apart from a real argument
    /// separator, the same disambiguation rustc itself performs, instead
    /// of reinventing it: a hand-rolled `<`/`>` depth counter can't, since
    /// those characters are also the comparison/shift operators
    /// (`assert!(a < b, "msg")` must still split into two arguments).
    #[instrument(level = "debug", skip(self, node))]
    fn check_macro_call(&mut self, node: &syn::Macro) {
        if !self.in_kani_proof {
            return;
        }
        let path = &node.path;
        if !(path.is_ident("assert") || path.is_ident("assert_eq")) {
            return;
        }
        let Ok(args) = Punctuated::<Expr, Token![,]>::parse_terminated.parse2(node.tokens.clone())
        else {
            // Not a shape this parses as plain Rust expressions -- skip
            // rather than guess.
            return;
        };

        let (first_expr, clause): (&Expr, TokenStream) = if path.is_ident("assert") {
            // Not a plain assert!(expr) or assert!(expr, "msg") shape —
            // skip rather than guess.
            if args.len() > 2 {
                return;
            }
            let Some(expr) = args.first() else {
                return;
            };
            (expr, expr.to_token_stream())
        } else {
            if args.len() < 2 {
                return;
            }
            let mut clause = args[0].to_token_stream();
            clause.extend(match "==".parse::<TokenStream>() {
                Ok(tokens) => tokens,
                Err(err) => {
                    self.error = Some(err.into());
                    return;
                }
            });
            clause.extend(args[1].to_token_stream());
            (&args[0], clause)
        };

        let line = first_expr.span().start().line as u32;
        self.check_clause("ensures", clause, line);
    }

    /// `kani::assume(EXPR)` — the requires-equivalent. Unlike
    /// `assert!`/`assert_eq!`, this is a plain function call (`assume`
    /// has no `!`), never a macro invocation, so it needs its own
    /// `syn::ExprCall` visitor rather than living in [`Self::check_macro_call`].
    #[instrument(level = "debug", skip(self, node))]
    fn check_assume_call(&mut self, node: &syn::ExprCall) {
        if !self.in_kani_proof {
            return;
        }
        let syn::Expr::Path(func) = node.func.as_ref() else {
            return;
        };
        let path = &func.path;
        let is_assume = path
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "assume")
            && path.segments.iter().any(|seg| seg.ident == "kani");
        if !is_assume {
            return;
        }
        let Some(expr) = node.args.first() else {
            return;
        };
        let line = expr.span().start().line as u32;
        self.check_clause("requires", expr.to_token_stream(), line);
    }
}

impl<'ast> Visit<'ast> for KaniVisitor<'_> {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if self.error.is_some() {
            return;
        }
        let prev = self.in_kani_proof;
        self.in_kani_proof = Self::is_kani_proof(&node.attrs);
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
        self.in_kani_proof = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if self.error.is_some() {
            return;
        }
        self.check_macro_call(node);
        syn::visit::visit_macro(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if self.error.is_some() {
            return;
        }
        self.check_assume_call(node);
        syn::visit::visit_expr_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_macro(&mut self, node: &'ast ItemMacro) {
        if self.error.is_some() {
            return;
        }
        if let Some(item_fn) = harness_macro_item_fn(node) {
            self.visit_item_fn(&item_fn);
        }
        syn::visit::visit_item_macro(self, node);
    }
}
