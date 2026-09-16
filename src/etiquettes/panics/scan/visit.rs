use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{ExprLit, ExprMacro, ExprMethodCall, Item, ItemFn, ItemImpl, ItemMod, Lit, Macro};

use super::snippets::{
    expect_snippet, has_cfg_flag, has_cfg_not_flag, is_cfg_test, is_unwrap_variant,
    macro_panic_kind, macro_snippet, type_label,
};
use super::{PanicKind, PanicScanVisitor};
use crate::etiquettes::panics::error_assertion;

use tracing::instrument;

impl PanicScanVisitor<'_> {
    #[instrument(level = "debug", skip(self, mac))]
    fn check_macro(&mut self, mac: &Macro) {
        if mac.path.is_ident("verus") {
            // When the `verus_ir` feature is available, `scan_crate_panics`/
            // `scan_rust_source` already merge in real, complete findings
            // from a genuine `verus_syn` parse (see this module's own
            // `verus_ir_findings`) -- skip the best-effort recovery here
            // entirely rather than double-count.
            #[cfg(not(feature = "verus_ir"))]
            self.scan_verus_chunks(super::verus_recovery::collect_functions(mac.tokens.clone()));
            return;
        }
        let Some(kind) = macro_panic_kind(&mac.path) else {
            return;
        };
        self.push_finding(kind, mac.span().start().line as u32, macro_snippet(mac));
    }

    #[instrument(level = "debug", skip(self, call))]
    fn check_method_call(&mut self, call: &ExprMethodCall) {
        let method = call.method.to_string();
        let line = call.span().start().line as u32;
        // Asserting a fact against the extracted error value (`let error
        // = ...expect_err(..); assert_eq!(error, ..)`) is that
        // assertion's own mechanism, not a discarded/propagated setup
        // failure -- see error_assertion's own doc comment. Only applies
        // to the `_err` methods: `.expect(..)`/`.unwrap()` assert the Ok
        // case succeeded, an entirely different situation this pattern
        // doesn't cover.
        let is_error_assertion = matches!(method.as_str(), "expect_err" | "unwrap_err")
            && self.exempt_error_assertion_lines.contains(&line);
        match method.as_str() {
            "expect" | "expect_err" if !is_error_assertion => {
                self.push_finding(PanicKind::Expect, line, expect_snippet(call))
            }
            "unwrap" | "unwrap_err" if !is_unwrap_variant(call) && !is_error_assertion => {
                self.push_finding(PanicKind::Unwrap, line, format!(".{}()", call.method))
            }
            _ => {}
        }
    }

    #[instrument(level = "debug", skip(self, items))]
    fn visit_module_items(&mut self, items: &[Item], module_prefix: &[String]) {
        let prev_prefix = self.module_prefix.clone();
        self.module_prefix = module_prefix.to_vec();
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.module_prefix = prev_prefix;
    }

    #[instrument(level = "debug", skip(self, item_mod))]
    fn visit_mod(&mut self, item_mod: &ItemMod) {
        let prev = self.in_cfg_test;
        if is_cfg_test(&item_mod.attrs) {
            self.in_cfg_test = true;
        }
        let Some((_, items)) = &item_mod.content else {
            self.in_cfg_test = prev;
            return;
        };
        let mut nested = self.module_prefix.clone();
        nested.push(item_mod.ident.to_string());
        self.visit_module_items(items, &nested);
        self.in_cfg_test = prev;
    }
}

impl<'ast> Visit<'ast> for PanicScanVisitor<'_> {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        self.visit_mod(node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let prev = self.in_cfg_test;
        if is_cfg_test(&node.attrs) {
            self.in_cfg_test = true;
        }
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
        self.in_cfg_test = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let prev_cfg = self.in_cfg_test;
        if is_cfg_test(&node.attrs) {
            self.in_cfg_test = true;
        }
        let prev = self.impl_type.clone();
        self.impl_type = Some(type_label(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        self.impl_type = prev;
        self.in_cfg_test = prev_cfg;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let prev = self.in_cfg_test;
        if is_cfg_test(&node.attrs) {
            self.in_cfg_test = true;
        }
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
        self.in_cfg_test = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        self.check_macro(&node.mac);
        syn::visit::visit_stmt_macro(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        self.check_macro(&node.mac);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_macro(&mut self, node: &'ast ExprMacro) {
        self.check_macro(&node.mac);
        syn::visit::visit_expr_macro(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        self.check_method_call(node);
        syn::visit::visit_expr_method_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_lit(&mut self, node: &'ast ExprLit) {
        if let Lit::Str(lit) = &node.lit {
            self.scan_embedded_source(lit);
        }
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_block(&mut self, node: &'ast syn::ExprBlock) {
        let prev = self.in_cfg_not_kani;
        if has_cfg_not_flag(&node.attrs, "kani") {
            self.in_cfg_not_kani = true;
        } else if has_cfg_flag(&node.attrs, "kani") {
            self.in_cfg_not_kani = false;
        }
        syn::visit::visit_expr_block(self, node);
        self.in_cfg_not_kani = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_block(&mut self, node: &'ast syn::Block) {
        self.exempt_error_assertion_lines
            .extend(error_assertion::error_assertion_lines(node));
        syn::visit::visit_block(self, node);
    }
}
