//! Extract [`VerusFnFacts`] from real `verus_syn`-parsed items.

use verus_syn::visit::Visit;

use crate::objects::FileSpan;

use super::parse::VerusBlock;
use super::types::{
    VerusCrateIr, VerusEnumFacts, VerusEnumVariantFacts, VerusFnFacts, VerusFnMode, VerusPanicKind,
    VerusPanicSite, VerusPublish,
};

use tracing::instrument;

/// Build a crate's [`VerusCrateIr`] from every block `collect_verus_blocks`
/// found.
#[instrument(level = "debug", skip(blocks))]
pub(super) fn build_crate_ir(blocks: Vec<VerusBlock>) -> VerusCrateIr {
    let mut functions = Vec::new();
    let mut enums = Vec::new();
    for block in blocks {
        let mut visitor = FactsVisitor {
            file: block.file,
            module_path: block.module_path,
            cfg_test: block.cfg_test,
            functions: Vec::new(),
            enums: Vec::new(),
        };
        for item in &block.items {
            visitor.visit_item(item);
        }
        functions.extend(visitor.functions);
        enums.extend(visitor.enums);
    }
    VerusCrateIr { functions, enums }
}

struct FactsVisitor {
    file: std::path::PathBuf,
    module_path: String,
    cfg_test: bool,
    functions: Vec<VerusFnFacts>,
    enums: Vec<VerusEnumFacts>,
}

impl FactsVisitor {
    #[instrument(level = "trace", skip(self, attrs, sig, block))]
    fn record(
        &mut self,
        attrs: &[verus_syn::Attribute],
        sig: &verus_syn::Signature,
        block: &verus_syn::Block,
        line: u32,
    ) {
        let spec = &sig.spec;
        let requires = spec
            .requires
            .as_ref()
            .map(|r| render_specification(&r.exprs))
            .unwrap_or_default();
        let ensures = spec
            .ensures
            .as_ref()
            .map(|e| render_specification(&e.exprs))
            .unwrap_or_default();
        let decreases = spec
            .decreases
            .as_ref()
            .map(|d| render_specification(&d.decreases.exprs).join(", "))
            .filter(|text| !text.is_empty());
        let recommends = spec
            .recommends
            .as_ref()
            .map(|r| render_specification(&r.exprs))
            .unwrap_or_default();

        let body = scan_body(block);

        self.functions.push(VerusFnFacts {
            name: sig.ident.to_string(),
            module_path: self.module_path.clone(),
            span: FileSpan::new(self.file.clone(), line, 0),
            cfg_test: self.cfg_test,
            mode: fn_mode(&sig.mode),
            publish: publish_kind(&sig.publish),
            requires,
            ensures,
            decreases,
            uses_assume: body.uses_assume,
            uses_admit: body.uses_admit,
            is_external_body: has_external_body(attrs),
            panic_sites: body.panic_sites,
            tracked_params: tracked_param_names(sig),
            recommends,
            is_broadcast: sig.broadcast.is_some(),
            calls: body.calls,
        });
    }
}

impl<'ast> Visit<'ast> for FactsVisitor {
    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast verus_syn::ItemFn) {
        let line = item_fn_line(node);
        self.record(&node.attrs, &node.sig, &node.block, line);
        verus_syn::visit::visit_item_fn(self, node);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast verus_syn::ImplItemFn) {
        let line = impl_item_fn_line(node);
        self.record(&node.attrs, &node.sig, &node.block, line);
        verus_syn::visit::visit_impl_item_fn(self, node);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_item_enum(&mut self, node: &'ast verus_syn::ItemEnum) {
        use verus_syn::spanned::Spanned;
        let line = node.enum_token.span().start().line as u32;
        self.enums.push(VerusEnumFacts {
            name: node.ident.to_string(),
            module_path: self.module_path.clone(),
            span: FileSpan::new(self.file.clone(), line, 0),
            cfg_test: self.cfg_test,
            has_doc: has_doc_comment(&node.attrs),
            variants: node.variants.iter().map(variant_facts).collect(),
        });
        verus_syn::visit::visit_item_enum(self, node);
    }
}

/// Real facts for one enum variant -- see [`VerusEnumFacts`]'s own doc
/// comment for why `carries_data`/`has_doc` are the two that matter.
#[instrument(level = "trace", skip(variant))]
fn variant_facts(variant: &verus_syn::Variant) -> VerusEnumVariantFacts {
    VerusEnumVariantFacts {
        name: variant.ident.to_string(),
        carries_data: !matches!(variant.fields, verus_syn::Fields::Unit),
        has_doc: has_doc_comment(&variant.attrs),
    }
}

/// Whether `attrs` carries a doc comment (`///`/`//!`, or a literal
/// `#[doc = ..]`) -- `///` desugars to `#[doc = "..."]` before this
/// visitor ever sees it, so a single check covers both spellings.
#[instrument(level = "trace", skip(attrs), ret)]
fn has_doc_comment(attrs: &[verus_syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("doc"))
}

#[instrument(level = "trace", skip(node), ret)]
fn item_fn_line(node: &verus_syn::ItemFn) -> u32 {
    use verus_syn::spanned::Spanned;
    node.sig.fn_token.span().start().line as u32
}

#[instrument(level = "trace", skip(node), ret)]
fn impl_item_fn_line(node: &verus_syn::ImplItemFn) -> u32 {
    use verus_syn::spanned::Spanned;
    node.sig.fn_token.span().start().line as u32
}

#[instrument(level = "trace", skip(exprs), ret)]
fn render_specification(exprs: &verus_syn::Specification) -> Vec<String> {
    exprs
        .exprs
        .iter()
        .map(|expr| {
            let tokens = quote::quote!(#expr);
            tokens.to_string()
        })
        .collect()
}

#[instrument(level = "trace", skip(mode), ret)]
fn fn_mode(mode: &verus_syn::FnMode) -> VerusFnMode {
    match mode {
        verus_syn::FnMode::Spec(_) => VerusFnMode::Spec,
        verus_syn::FnMode::SpecChecked(_) => VerusFnMode::SpecChecked,
        verus_syn::FnMode::Proof(_) => VerusFnMode::Proof,
        verus_syn::FnMode::ProofAxiom(_) => VerusFnMode::ProofAxiom,
        verus_syn::FnMode::Exec(_) => VerusFnMode::Exec,
        verus_syn::FnMode::Default => VerusFnMode::Default,
    }
}

#[instrument(level = "trace", skip(publish), ret)]
fn publish_kind(publish: &verus_syn::Publish) -> VerusPublish {
    match publish {
        verus_syn::Publish::Closed(_) => VerusPublish::Closed,
        verus_syn::Publish::Open(_) => VerusPublish::Open,
        verus_syn::Publish::OpenRestricted(_) => VerusPublish::OpenRestricted,
        verus_syn::Publish::Uninterp(_) => VerusPublish::Uninterp,
        verus_syn::Publish::Default => VerusPublish::Default,
    }
}

/// Whether `attrs` carries `#[verifier::external_body]` (any path whose
/// final segment is `external_body` -- covers both the fully-qualified
/// and any locally `use`-imported spelling).
#[instrument(level = "trace", skip(attrs), ret)]
fn has_external_body(attrs: &[verus_syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "external_body")
    })
}

/// Every name a `tracked` parameter is bound to -- see
/// [`VerusFnFacts::tracked_params`]'s own doc comment.
#[instrument(level = "trace", skip(sig), ret)]
fn tracked_param_names(sig: &verus_syn::Signature) -> Vec<String> {
    sig.inputs
        .iter()
        .filter(|arg| arg.tracked.is_some())
        .filter_map(|arg| match &arg.kind {
            verus_syn::FnArgKind::Typed(pat_type) => param_name(&pat_type.pat),
            verus_syn::FnArgKind::Receiver(_) => None,
        })
        .collect()
}

/// The bound identifier a simple `pat` names, if it's a plain identifier
/// pattern (not a tuple/struct destructure).
#[instrument(level = "trace", skip(pat), ret)]
fn param_name(pat: &verus_syn::Pat) -> Option<String> {
    match pat {
        verus_syn::Pat::Ident(pat_ident) => Some(pat_ident.ident.to_string()),
        _ => None,
    }
}

struct BodyFacts {
    uses_assume: bool,
    uses_admit: bool,
    panic_sites: Vec<VerusPanicSite>,
    calls: Vec<String>,
}

/// Walk `block` once for every real, local fact its body carries: real
/// `assume(..)`/`admit()` soundness escape hatches (see
/// [`VerusFnFacts::uses_assume`]/[`VerusFnFacts::uses_admit`]'s own doc
/// comments), every `panic!`/`unreachable!`/`.expect(..)`/`.unwrap()`
/// abort site, and every local call's bare target name (see
/// [`VerusFnFacts::calls`]).
#[instrument(level = "trace", skip(block))]
fn scan_body(block: &verus_syn::Block) -> BodyFacts {
    let mut visitor = BodyVisitor {
        uses_assume: false,
        uses_admit: false,
        panic_sites: Vec::new(),
        in_proven_unreachable_arm: false,
        calls: Vec::new(),
    };
    visitor.visit_block(block);
    BodyFacts {
        uses_assume: visitor.uses_assume,
        uses_admit: visitor.uses_admit,
        panic_sites: visitor.panic_sites,
        calls: visitor.calls,
    }
}

struct BodyVisitor {
    uses_assume: bool,
    uses_admit: bool,
    panic_sites: Vec<VerusPanicSite>,
    /// Set for the duration of visiting a `match` arm's guard/body when
    /// that arm is `#[cfg(not(verus_keep_ghost))]`-gated and a sibling
    /// arm -- same pattern, `#[cfg(verus_keep_ghost)]`-gated -- calls
    /// `unreached()`. See [`VerusPanicSite::proven_unreachable_by_ghost_sibling`].
    in_proven_unreachable_arm: bool,
    /// The bare name of every function/method call this body makes --
    /// see [`VerusFnFacts::calls`].
    calls: Vec<String>,
}

impl BodyVisitor {
    #[instrument(level = "trace", skip(self, mac))]
    fn record_macro(&mut self, mac: &verus_syn::Macro) {
        use verus_syn::spanned::Spanned;
        let Some(segment) = mac.path.segments.last() else {
            return;
        };
        let kind = match segment.ident.to_string().as_str() {
            "panic" => Some(VerusPanicKind::Panic),
            "unreachable" => Some(VerusPanicKind::Unreachable),
            "compile_error" => Some(VerusPanicKind::CompileError),
            _ => None,
        };
        if let Some(kind) = kind {
            self.panic_sites.push(VerusPanicSite {
                kind,
                line: mac.span().start().line as u32,
                snippet: format!("{}!(..)", segment.ident),
                proven_unreachable_by_ghost_sibling: self.in_proven_unreachable_arm,
            });
        }
    }
}

impl<'ast> Visit<'ast> for BodyVisitor {
    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr(&mut self, node: &'ast verus_syn::Expr) {
        if matches!(node, verus_syn::Expr::Assume(_)) {
            self.uses_assume = true;
        }
        verus_syn::visit::visit_expr(self, node);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast verus_syn::ExprCall) {
        if let verus_syn::Expr::Path(path) = node.func.as_ref()
            && let Some(segment) = path.path.segments.last()
        {
            if segment.ident == "admit" {
                self.uses_admit = true;
            }
            self.calls.push(segment.ident.to_string());
        }
        verus_syn::visit::visit_expr_call(self, node);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_macro(&mut self, node: &'ast verus_syn::ExprMacro) {
        self.record_macro(&node.mac);
        verus_syn::visit::visit_expr_macro(self, node);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_stmt_macro(&mut self, node: &'ast verus_syn::StmtMacro) {
        self.record_macro(&node.mac);
        verus_syn::visit::visit_stmt_macro(self, node);
    }

    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast verus_syn::ExprMethodCall) {
        self.calls.push(node.method.to_string());
        let kind = match node.method.to_string().as_str() {
            "expect" => Some(VerusPanicKind::Expect),
            "unwrap" | "unwrap_err" => Some(VerusPanicKind::Unwrap),
            _ => None,
        };
        if let Some(kind) = kind {
            self.panic_sites.push(VerusPanicSite {
                kind,
                line: node.method.span().start().line as u32,
                snippet: format!(".{}(..)", node.method),
                proven_unreachable_by_ghost_sibling: self.in_proven_unreachable_arm,
            });
        }
        verus_syn::visit::visit_expr_method_call(self, node);
    }

    /// Overrides the default traversal (rather than delegating to
    /// `verus_syn::visit::visit_expr_match`) so the ghost/exec sibling
    /// pairing can be computed once, up front, from every arm before any
    /// arm's own body is visited -- a per-arm override alone has no way
    /// to see its siblings.
    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_match(&mut self, node: &'ast verus_syn::ExprMatch) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        self.visit_expr(&node.expr);

        let ghost_unreached_patterns: std::collections::HashSet<String> = node
            .arms
            .iter()
            .filter(|arm| {
                has_cfg_flag(&arm.attrs, "verus_keep_ghost") && is_bare_unreached_call(&arm.body)
            })
            .map(|arm| pattern_key(&arm.pat))
            .collect();

        for arm in &node.arms {
            for attr in &arm.attrs {
                self.visit_attribute(attr);
            }
            self.visit_pat(&arm.pat);
            if let Some((_, guard)) = &arm.guard {
                self.visit_expr(guard);
            }
            let exempt = has_cfg_not_flag(&arm.attrs, "verus_keep_ghost")
                && ghost_unreached_patterns.contains(&pattern_key(&arm.pat));
            let prev = self.in_proven_unreachable_arm;
            if exempt {
                self.in_proven_unreachable_arm = true;
            }
            self.visit_expr(&arm.body);
            self.in_proven_unreachable_arm = prev;
        }
    }
}

/// Whether `attrs` carries a bare `#[cfg(flag)]` (no `not(..)`, no
/// `all(..)`/`any(..)` combinators -- just the single flag name).
#[instrument(level = "trace", skip(attrs, flag))]
fn has_cfg_flag(attrs: &[verus_syn::Attribute], flag: &str) -> bool {
    attrs.iter().any(|attr| {
        let verus_syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        list.path.is_ident("cfg") && list.tokens.to_string().replace(' ', "") == flag
    })
}

/// Whether `attrs` carries a bare `#[cfg(not(flag))]`.
#[instrument(level = "trace", skip(attrs, flag))]
fn has_cfg_not_flag(attrs: &[verus_syn::Attribute], flag: &str) -> bool {
    attrs.iter().any(|attr| {
        let verus_syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        list.path.is_ident("cfg")
            && list.tokens.to_string().replace(' ', "") == format!("not({flag})")
    })
}

/// Whether `expr` is a bare `unreached()` call -- deliberately narrow
/// (no block-wrapped/nested-call recognition): the only real shape this
/// codebase's own `verus_keep_ghost` split uses, and the safe direction
/// for a pattern feeding an exemption is to under-match (a real ghost
/// sibling goes unrecognized, so its exec fallback just stays flagged a
/// little longer) rather than over-match (which could wrongly mark a
/// real panic site as proven-impossible).
#[instrument(level = "trace", skip(expr), ret)]
fn is_bare_unreached_call(expr: &verus_syn::Expr) -> bool {
    let verus_syn::Expr::Call(call) = expr else {
        return false;
    };
    let verus_syn::Expr::Path(path) = call.func.as_ref() else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "unreached")
}

/// Textual key for comparing two arms' patterns for equality (`Ok(_)` ==
/// `Ok(_)`) without needing real semantic pattern equality.
#[instrument(level = "trace", skip(pat), ret)]
fn pattern_key(pat: &verus_syn::Pat) -> String {
    quote::quote!(#pat).to_string()
}
