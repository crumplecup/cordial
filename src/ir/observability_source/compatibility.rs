//! Compatibility projection at the shared loader boundary; never reparses a file.
use super::{
    ObservabilityBody, ObservabilityFile, ObservabilityFileKind, ObservabilityFunction,
    ObservabilityPrint, cargo_protocol, print_rule,
};
use crate::FileSnapshot;
use crate::ir::SourceInstrumentSyntax;
use crate::loader::{SharedParsedFile, module_path_from_src_file};
use std::path::Path;
use syn::{spanned::Spanned, visit::Visit};

#[tracing::instrument(level = "debug", skip(file))]
pub(super) fn collect(file: &SharedParsedFile, tree: &Path, root: &Path) -> ObservabilityFile {
    let mut visitor = Collector {
        modules: Vec::new(),
        print_prefix: module_path_from_src_file(tree, file.path()),
        functions: Vec::new(),
        prints: Vec::new(),
    };
    visitor.visit_file(file.syntax());
    ObservabilityFile {
        snapshot: FileSnapshot::new(file.path().to_path_buf(), file.source_digest()),
        relative: file
            .path()
            .strip_prefix(root)
            .unwrap_or(file.path())
            .to_path_buf(),
        kind: ObservabilityFileKind::for_path(file.path(), root),
        syntax_valid: true,
        functions: visitor.functions,
        prints: visitor.prints,
    }
}

struct Collector {
    modules: Vec<String>,
    print_prefix: Vec<String>,
    functions: Vec<ObservabilityFunction>,
    prints: Vec<ObservabilityPrint>,
}

impl Collector {
    #[tracing::instrument(level = "debug", skip(self, sig, attrs, block))]
    fn function(
        &mut self,
        sig: &syn::Signature,
        attrs: &[syn::Attribute],
        block: &syn::Block,
        line: u32,
    ) {
        let mut body = BodyCollector {
            facts: ObservabilityBody {
                is_fallible: match &sig.output {
                    syn::ReturnType::Type(_, ty) => is_result(ty),
                    _ => false,
                },
                has_err_instrument: attrs
                    .iter()
                    .filter_map(SourceInstrumentSyntax::from_attribute)
                    .any(|syntax| super::has_err_instrument(&syntax)),
                ..ObservabilityBody::default()
            },
        };
        body.visit_block(block);
        self.functions.push(ObservabilityFunction {
            name: sig.ident.to_string(),
            modules: self.modules.clone(),
            line,
            test: attrs.iter().any(|attr| {
                attr.path()
                    .segments
                    .last()
                    .is_some_and(|seg| seg.ident == "test")
            }),
            body: body.facts,
        });
    }
}

impl<'ast> Visit<'ast> for Collector {
    #[tracing::instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if let Some((_, items)) = &node.content {
            self.modules.push(node.ident.to_string());
            for item in items {
                self.visit_item(item);
            }
            self.modules.pop();
        }
    }
    #[tracing::instrument(level = "debug", skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.function(
            &node.sig,
            &node.attrs,
            &node.block,
            node.span().start().line as u32,
        );
        syn::visit::visit_item_fn(self, node);
    }
    #[tracing::instrument(level = "debug", skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.function(
            &node.sig,
            &node.attrs,
            &node.block,
            node.span().start().line as u32,
        );
        syn::visit::visit_impl_item_fn(self, node);
    }
    #[tracing::instrument(level = "debug", skip(self, node))]
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if let Some(rule) = node
            .path
            .segments
            .last()
            .and_then(|seg| print_rule(&seg.ident.to_string()))
        {
            let modules: Vec<_> = self
                .print_prefix
                .iter()
                .chain(&self.modules)
                .cloned()
                .collect();
            self.prints.push(ObservabilityPrint {
                context: if modules.is_empty() {
                    "<crate>".into()
                } else {
                    modules.join("::")
                },
                line: node.path.span().start().line as u32,
                rule,
                cargo_protocol: cargo_protocol(&node.tokens.to_string()),
            });
        }
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
fn is_result(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(ty) => ty
            .path
            .segments
            .last()
            .is_some_and(|seg| seg.ident.to_string().ends_with("Result")),
        syn::Type::Reference(ty) => is_result(&ty.elem),
        syn::Type::Paren(ty) => is_result(&ty.elem),
        _ => false,
    }
}

struct BodyCollector {
    facts: ObservabilityBody,
}
impl<'ast> Visit<'ast> for BodyCollector {
    #[tracing::instrument(level = "debug", skip(self, _node))]
    fn visit_item_fn(&mut self, _node: &'ast syn::ItemFn) {}
    #[tracing::instrument(level = "debug", skip(self, _node))]
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    #[tracing::instrument(level = "debug", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(expr) = node.func.as_ref()
            && let Some(seg) = expr.path.segments.last()
        {
            self.facts.note_call(&seg.ident.to_string(), false);
            self.facts.called_paths.push(
                expr.path
                    .segments
                    .iter()
                    .map(|seg| seg.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            );
        }
        syn::visit::visit_expr_call(self, node);
    }
    #[tracing::instrument(level = "debug", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.facts.note_call(&node.method.to_string(), true);
        syn::visit::visit_expr_method_call(self, node);
    }
    #[tracing::instrument(level = "debug", skip(self, node))]
    fn visit_path(&mut self, node: &'ast syn::Path) {
        if let Some(seg) = node.segments.last() {
            self.facts.note_path(&seg.ident.to_string());
        }
        syn::visit::visit_path(self, node);
    }
    #[tracing::instrument(level = "debug", skip(self, node))]
    fn visit_expr_lit(&mut self, node: &'ast syn::ExprLit) {
        if let syn::Lit::Str(value) = &node.lit {
            self.facts.has_rust_log_literal |= value.value() == "RUST_LOG";
        }
    }
    #[tracing::instrument(level = "debug", skip(self, node))]
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.facts.has_error_emission |= node
            .path
            .segments
            .last()
            .is_some_and(|seg| matches!(seg.ident.to_string().as_str(), "warn" | "error"));
        syn::visit::visit_macro(self, node);
    }
}
