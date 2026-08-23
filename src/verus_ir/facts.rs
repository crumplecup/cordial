//! Extract [`VerusFnFacts`] from real `verus_syn`-parsed items.

use verus_syn::visit::Visit;

use crate::objects::FileSpan;

use super::parse::VerusBlock;
use super::types::{
    VerusCrateIr, VerusFnFacts, VerusFnMode, VerusPanicKind, VerusPanicSite, VerusPublish,
};

use tracing::instrument;

/// Build a crate's [`VerusCrateIr`] from every block `collect_verus_blocks`
/// found.
#[instrument(level = "debug", skip(blocks))]
pub(super) fn build_crate_ir(blocks: Vec<VerusBlock>) -> VerusCrateIr {
    let mut functions = Vec::new();
    for block in blocks {
        let mut visitor = FactsVisitor {
            file: block.file,
            module_path: block.module_path,
            cfg_test: block.cfg_test,
            functions: Vec::new(),
        };
        for item in &block.items {
            visitor.visit_item(item);
        }
        functions.extend(visitor.functions);
    }
    VerusCrateIr { functions }
}

struct FactsVisitor {
    file: std::path::PathBuf,
    module_path: String,
    cfg_test: bool,
    functions: Vec<VerusFnFacts>,
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
}

/// Walk `block` once for every real, local fact its body carries: real
/// `assume(..)`/`admit()` soundness escape hatches (see
/// [`VerusFnFacts::uses_assume`]/[`VerusFnFacts::uses_admit`]'s own doc
/// comments), and every `panic!`/`unreachable!`/`.expect(..)`/
/// `.unwrap()` abort site -- the direct completion of this module's own
/// motivating gap.
#[instrument(level = "trace", skip(block))]
fn scan_body(block: &verus_syn::Block) -> BodyFacts {
    let mut visitor = BodyVisitor {
        uses_assume: false,
        uses_admit: false,
        panic_sites: Vec::new(),
    };
    visitor.visit_block(block);
    BodyFacts {
        uses_assume: visitor.uses_assume,
        uses_admit: visitor.uses_admit,
        panic_sites: visitor.panic_sites,
    }
}

struct BodyVisitor {
    uses_assume: bool,
    uses_admit: bool,
    panic_sites: Vec<VerusPanicSite>,
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
            && path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "admit")
        {
            self.uses_admit = true;
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
            });
        }
        verus_syn::visit::visit_expr_method_call(self, node);
    }
}
