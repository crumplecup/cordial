//! Extract [`VerusFnFacts`] from real `verus_syn`-parsed items.

use verus_syn::visit::Visit;

use crate::objects::FileSpan;

use super::parse::VerusBlock;
use super::types::{VerusCrateIr, VerusFnFacts, VerusFnMode, VerusPublish};

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

        let (uses_assume, uses_admit) = scan_body_for_escape_hatches(block);

        self.functions.push(VerusFnFacts {
            name: sig.ident.to_string(),
            module_path: self.module_path.clone(),
            span: FileSpan::new(self.file.clone(), line, 0),
            mode: fn_mode(&sig.mode),
            publish: publish_kind(&sig.publish),
            requires,
            ensures,
            decreases,
            uses_assume,
            uses_admit,
            is_external_body: has_external_body(attrs),
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

/// Whether `block`'s body calls `assume(..)` (real, dedicated Verus
/// syntax) or `admit()` (an ordinary call to a well-known Verus builtin
/// -- no dedicated syntax of its own) anywhere within it.
#[instrument(level = "trace", skip(block), ret)]
fn scan_body_for_escape_hatches(block: &verus_syn::Block) -> (bool, bool) {
    let mut visitor = EscapeHatchVisitor {
        uses_assume: false,
        uses_admit: false,
    };
    visitor.visit_block(block);
    (visitor.uses_assume, visitor.uses_admit)
}

struct EscapeHatchVisitor {
    uses_assume: bool,
    uses_admit: bool,
}

impl<'ast> Visit<'ast> for EscapeHatchVisitor {
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
}
