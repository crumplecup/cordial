use syn::visit::Visit;
use syn::{Block, ExprCall, ExprLit, ExprMethodCall, ExprPath, ImplItemFn, ItemFn, Path};

use tracing::instrument;

/// Facts collected from one function body about subscriber install.
#[derive(Debug, Default, Clone)]
pub(super) struct InitBodyFacts {
    /// Body itself contains real install code: a direct `init`/`try_init`/
    /// `set_global_default` call. Does **not** include a call matching
    /// `known_helper_paths` -- see [`Self::calls_known_helper`] and
    /// [`Self::installs_or_delegates`] for that.
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
    /// Body calls a path matching `known_helper_paths` (a cross-crate
    /// shared helper this crate can't see the definition of -- see
    /// [`Self::from_block`]). Kept separate from [`Self::calls_install`]:
    /// a call delegating to a *documented, named* helper elsewhere is
    /// exactly what `helper_in_lib` wants, not the antipattern it flags,
    /// so the `Lib`/`RustLog`/`Idempotent` rules must never treat one as
    /// "install code inlined here" -- only `Main`/`Test` should accept it.
    pub calls_known_helper: bool,
    /// Cross-crate helper paths (from `cordial.toml`'s
    /// `[tracing.subscriber] known_helper_paths`) that count as a real
    /// install even though this crate's own scan never sees their body --
    /// a shared helper living in one crate (e.g. `amenable_core::
    /// init_tracing`) called from a sibling crate's `main`/`#[test]` has no
    /// other way to be recognized, since [`super::scan::scan_crate_tracing_subscriber`]
    /// only scans one crate's own source tree.
    known_helper_paths: Vec<String>,
}

impl InitBodyFacts {
    #[instrument(level = "debug", skip(known_helper_paths))]
    pub(super) fn from_block(block: &Block, known_helper_paths: &[String]) -> Self {
        let mut facts = Self {
            known_helper_paths: known_helper_paths.to_vec(),
            ..Self::default()
        };
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

    /// What `Main`/`Test` actually require: either real install code
    /// inline, or a documented call to a configured cross-crate helper.
    #[instrument(level = "debug", skip(self))]
    pub(super) fn installs_or_delegates(&self) -> bool {
        self.calls_install || self.calls_known_helper
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

/// `::`-joined segment text, e.g. `amenable_core::init_tracing` -- for
/// matching a call site's full path against a configured cross-crate
/// helper (bare last-segment matching alone can't tell `foo::init_tracing`
/// from an unrelated `init_tracing` in a different module).
#[instrument(level = "debug", skip(path))]
fn path_joined(path: &Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
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

    /// A call whose full path (or bare last segment) matches a configured
    /// cross-crate helper counts as a complete, trusted install: this
    /// crate's own scan can't see the helper's body to verify RUST_LOG/
    /// idempotency itself, so it trusts the config declaration -- the
    /// helper's *own* defining crate independently verifies its body when
    /// that crate is scanned (`helper_in_lib`/`rust_log_fallback`/
    /// `idempotent` still apply there).
    #[instrument(level = "debug", skip(self))]
    fn note_call_path(&mut self, full_path: &str) {
        let last = full_path.rsplit("::").next().unwrap_or(full_path);
        let is_known = self
            .known_helper_paths
            .iter()
            .any(|helper| helper == full_path || helper.rsplit("::").next() == Some(last));
        if !is_known {
            return;
        }
        self.calls_known_helper = true;
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
            self.note_call_path(&path_joined(path));
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
