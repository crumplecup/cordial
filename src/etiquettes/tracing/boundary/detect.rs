use syn::visit::Visit;
use syn::{
    Attribute, Block, ExprCall, ExprMethodCall, ExprPath, Macro, Meta, Path, ReturnType, Signature,
    Type, TypePath,
};

use tracing::instrument;

use crate::etiquettes::tracing::present::parse_instrument_meta;

/// Facts collected from one function's signature and body about whether it
/// reports its own error to the UI-facing tracing channel before returning.
#[derive(Debug, Default, Clone)]
pub(super) struct BoundaryBodyFacts {
    /// Return type is `Result<_, _>` (or a `*Result` alias).
    pub is_fallible: bool,
    /// `#[instrument(err(...))]` (or bare `err`) is present, including
    /// wrapped in `#[cfg_attr(pred, instrument(...))]`.
    pub has_err_instrument: bool,
    /// Body directly calls `tracing::warn!`/`tracing::error!` (bare
    /// `warn!`/`error!` also counts — this project's `use tracing::warn`
    /// convention is orthogonal to this check).
    pub has_error_emission: bool,
    /// Last path segment or method name of every call, for helper-name
    /// delegation matching (mirrors [`super::super::subscriber::detect::InitBodyFacts`]).
    pub called_names: Vec<String>,
    /// Body calls a path matching a configured cross-crate helper that
    /// already reports its own errors — trusted the same way subscriber
    /// trusts `known_helper_paths` for cross-crate init delegation.
    pub calls_known_helper: bool,
    known_helper_paths: Vec<String>,
}

impl BoundaryBodyFacts {
    #[instrument(level = "debug", skip(sig, attrs, block, known_helper_paths))]
    pub(super) fn from_fn(
        sig: &Signature,
        attrs: &[Attribute],
        block: &Block,
        known_helper_paths: &[String],
    ) -> Self {
        let mut facts = Self {
            is_fallible: sig_returns_result(sig),
            has_err_instrument: instrument_has_err(attrs),
            known_helper_paths: known_helper_paths.to_vec(),
            ..Self::default()
        };
        facts.visit_block(block);
        facts
    }

    /// Whether this function's own signature/body already directs an
    /// error to the UI-facing tracing channel — the binary-boundary
    /// policy this crate cares about, regardless of delegation.
    #[instrument(level = "debug", skip(self))]
    pub(super) fn reports_errors(&self) -> bool {
        self.has_err_instrument || self.has_error_emission || self.calls_known_helper
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn calls_safe_helper(&self, safe_names: &[&str]) -> bool {
        self.called_names
            .iter()
            .any(|name| safe_names.iter().any(|safe| safe == name))
    }
}

#[instrument(level = "debug", skip(sig))]
fn sig_returns_result(sig: &Signature) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => type_is_result(ty),
        ReturnType::Default => false,
    }
}

#[instrument(level = "debug", skip(ty))]
fn type_is_result(ty: &Type) -> bool {
    match ty {
        Type::Path(TypePath { path, .. }) => path.segments.last().is_some_and(|segment| {
            let ident = segment.ident.to_string();
            ident == "Result" || ident.ends_with("Result")
        }),
        Type::Reference(reference) => type_is_result(&reference.elem),
        Type::Paren(paren) => type_is_result(&paren.elem),
        _ => false,
    }
}

#[instrument(level = "debug", skip(path))]
fn path_last(path: &Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

/// Meta string for one attribute, in the same `name(tokens)` shape the
/// attribute enricher stores (`crate::enricher::attribute::attr_meta_string`
/// is private to that module, so this mirrors it locally rather than
/// widening its visibility for one caller).
#[instrument(level = "debug", skip(attr))]
fn attr_meta_string(attr: &Attribute) -> String {
    match &attr.meta {
        Meta::Path(path) => path_last(path).unwrap_or_default(),
        Meta::List(list) => format!(
            "{}({})",
            path_last(&list.path).unwrap_or_default(),
            list.tokens
        ),
        Meta::NameValue(value) => format!("{} = …", path_last(&value.path).unwrap_or_default()),
    }
}

/// First top-level-comma-split part after `#[cfg_attr(<predicate>, <inner>)]`'s
/// predicate, given just the `cfg_attr(...)` call's inner tokens rendered as
/// text (`not (kani) , instrument (...)`).
#[instrument(level = "debug")]
fn cfg_attr_inner(tokens: &str) -> Option<&str> {
    let mut depth: u32 = 0;
    for (idx, ch) in tokens.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some(tokens[idx + 1..].trim()),
            _ => {}
        }
    }
    None
}

/// Whether `attrs` carries `#[instrument(err(...))]` (bare `err` counts
/// too), including the `#[cfg_attr(pred, instrument(...))]` / `#[cfg_attr(pred,
/// tracing::instrument(...))]` gated forms `--apply` writes for verifier crates.
#[instrument(level = "debug", skip(attrs))]
fn instrument_has_err(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let Some(last) = path_last(attr.path()) else {
            return false;
        };
        if last == "instrument" {
            return parse_instrument_meta(&attr_meta_string(attr)).err();
        }
        if last != "cfg_attr" {
            return false;
        }
        let Meta::List(list) = &attr.meta else {
            return false;
        };
        let tokens = list.tokens.to_string();
        let Some(inner) = cfg_attr_inner(&tokens) else {
            return false;
        };
        let compact: String = inner.split_whitespace().collect();
        let rest = compact.strip_prefix("::").unwrap_or(compact.as_str());
        let rest = rest.strip_prefix("tracing::").unwrap_or(rest);
        rest.starts_with("instrument") && parse_instrument_meta(rest).err()
    })
}

impl BoundaryBodyFacts {
    #[instrument(level = "debug", skip(self, name))]
    fn note_call_name(&mut self, name: &str) {
        self.called_names.push(name.to_string());
    }

    /// A call whose full path (or bare last segment) matches a configured
    /// cross-crate helper is trusted as already reporting its own errors —
    /// this crate's own scan can't see the helper's body to verify it, so
    /// it trusts the config declaration, exactly like subscriber's
    /// `known_helper_paths`.
    #[instrument(level = "debug", skip(self, full_path))]
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

impl<'ast> Visit<'ast> for BoundaryBodyFacts {
    #[instrument(level = "debug", skip(self, _node))]
    fn visit_item_fn(&mut self, _node: &'ast syn::ItemFn) {
        // Nested functions are their own boundary sites.
    }

    #[instrument(level = "debug", skip(self, _node))]
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {
        // Nested inherent methods are their own boundary sites.
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let syn::Expr::Path(ExprPath { path, .. }) = node.func.as_ref()
            && let Some(name) = path_last(path)
        {
            self.note_call_name(&name);
            self.note_call_path(
                &path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            );
        }
        syn::visit::visit_expr_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        self.note_call_name(&node.method.to_string());
        syn::visit::visit_expr_method_call(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_macro(&mut self, node: &'ast Macro) {
        if let Some(name) = path_last(&node.path)
            && matches!(name.as_str(), "warn" | "error")
        {
            self.has_error_emission = true;
        }
        syn::visit::visit_macro(self, node);
    }
}
