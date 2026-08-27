use syn::visit::Visit;
use syn::{Block, ExprCall, ExprLit, ExprMethodCall, ExprPath, ImplItemFn, ItemFn, Path};

use tracing::instrument;

/// Facts collected from one function body about subscriber install.
#[derive(Debug, Default, Clone)]
pub(super) struct InitBodyFacts {
    /// Body calls `init` / `try_init` / `set_global_default`.
    pub calls_install: bool,
    /// Body calls `try_init` or `set_global_default` (already-set is an error, not a panic).
    pub calls_try_init: bool,
    /// Body calls bare `init` (panics if a subscriber is already set).
    pub calls_init: bool,
    /// Body names `Once` or `OnceLock`.
    pub has_once: bool,
    pub has_try_from_default_env: bool,
    pub has_rust_log_literal: bool,
    pub has_fallback: bool,
    /// Last path segment or method name of every call, for helper-name matching.
    pub called_names: Vec<String>,
}

impl InitBodyFacts {
    #[instrument(level = "debug")]
    pub(super) fn from_block(block: &Block) -> Self {
        let mut facts = Self::default();
        facts.visit_block(block);
        facts
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn rust_log_ok(&self) -> bool {
        (self.has_try_from_default_env && self.has_fallback)
            || (self.has_rust_log_literal && self.has_fallback)
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn idempotent_ok(&self) -> bool {
        self.calls_try_init || (self.has_once && self.calls_install)
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn calls_helper(&self, helpers: &[&str]) -> bool {
        self.called_names
            .iter()
            .any(|name| helpers.iter().any(|helper| helper == name))
    }
}

#[instrument(level = "debug")]
pub(super) fn is_install_name(name: &str) -> bool {
    matches!(name, "init" | "try_init" | "set_global_default")
}

#[instrument(level = "debug", skip(path))]
fn path_last(path: &Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

impl InitBodyFacts {
    #[instrument(level = "debug", skip(self))]
    fn note_call_name(&mut self, name: &str) {
        self.called_names.push(name.to_string());
        if !is_install_name(name) {
            return;
        }
        self.calls_install = true;
        match name {
            "try_init" | "set_global_default" => self.calls_try_init = true,
            "init" => self.calls_init = true,
            _ => {}
        }
    }
}

impl<'ast> Visit<'ast> for InitBodyFacts {
    #[instrument(level = "debug", skip(self, _node))]
    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {
        // Nested functions are their own init sites.
    }

    #[instrument(level = "debug", skip(self, _node))]
    fn visit_impl_item_fn(&mut self, _node: &'ast ImplItemFn) {
        // Nested inherent methods are their own init sites.
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let syn::Expr::Path(ExprPath { path, .. }) = node.func.as_ref()
            && let Some(name) = path_last(path)
        {
            self.note_call_name(&name);
            if name.as_str() == "try_from_default_env" {
                self.has_try_from_default_env = true;
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let name = node.method.to_string();
        self.note_call_name(&name);
        match name.as_str() {
            "try_from_default_env" => self.has_try_from_default_env = true,
            "unwrap_or" | "unwrap_or_else" | "unwrap_or_default" => self.has_fallback = true,
            _ => {}
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_path(&mut self, node: &'ast Path) {
        if let Some(name) = path_last(node) {
            match name.as_str() {
                "Once" | "OnceLock" => self.has_once = true,
                "try_from_default_env" => self.has_try_from_default_env = true,
                _ => {}
            }
        }
        syn::visit::visit_path(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_lit(&mut self, node: &'ast ExprLit) {
        if let syn::Lit::Str(value) = &node.lit
            && value.value() == "RUST_LOG"
        {
            self.has_rust_log_literal = true;
        }
        syn::visit::visit_expr_lit(self, node);
    }
}
