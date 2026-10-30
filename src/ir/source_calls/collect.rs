//! Collect namespace and use evidence in one shared-tree traversal.

use syn::{spanned::Spanned, visit::Visit};

use super::{SourceByteRange, SourceModuleScope, SourcePathUse, SourceUseInventory};

#[tracing::instrument(level = "trace", skip(file))]
pub(super) fn collect(file: &crate::loader::SharedParsedFile) -> SourceModuleScope {
    let mut collector = Collector {
        scope: SourceModuleScope {
            source_digest: file.source_digest(),
            insertion_offset: file.source().len(),
            identifiers: Vec::new(),
            opaque: false,
            extern_crates: false,
            uses: None,
        },
        uses: SourceUseInventory {
            paths: Vec::new(),
            opaque_ranges: Vec::new(),
        },
        item_depth: 0,
        owner_signature: None,
        nested_scope: false,
        call_range: None,
    };
    collector.visit_file(file.syntax());
    collector.scope.identifiers.sort();
    collector.scope.identifiers.dedup();
    collector.scope.uses = Some(collector.uses);
    collector.scope
}

struct Collector {
    scope: SourceModuleScope,
    uses: SourceUseInventory,
    item_depth: usize,
    owner_signature: Option<SourceByteRange>,
    nested_scope: bool,
    call_range: Option<SourceByteRange>,
}

impl<'ast> Visit<'ast> for Collector {
    #[tracing::instrument(level = "trace", skip(self, ident))]
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        self.scope
            .identifiers
            .push(ident.to_string().trim_start_matches("r#").to_owned());
    }

    #[tracing::instrument(level = "trace", skip(self, attribute))]
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        self.scope.opaque |= !attribute.path().is_ident("doc");
    }

    #[tracing::instrument(level = "trace", skip(self, item))]
    fn visit_item(&mut self, item: &'ast syn::Item) {
        self.scope.opaque |= matches!(
            item,
            syn::Item::Use(_)
                | syn::Item::ExternCrate(_)
                | syn::Item::Macro(_)
                | syn::Item::Verbatim(_)
        );
        self.scope.extern_crates |= matches!(item, syn::Item::ExternCrate(_));
        let previous_owner = self.owner_signature.clone();
        let previous_nested = self.nested_scope;
        if self.item_depth == 0 {
            self.owner_signature = match item {
                syn::Item::Fn(function) => Some(function.sig.span().byte_range().into()),
                _ => None,
            };
        } else {
            self.nested_scope = true;
        }
        if let syn::Item::Verbatim(tokens) = item {
            self.uses
                .opaque_ranges
                .push(tokens.span().byte_range().into());
        }
        self.item_depth += 1;
        syn::visit::visit_item(self, item);
        self.item_depth -= 1;
        self.owner_signature = previous_owner;
        self.nested_scope = previous_nested;
    }

    #[tracing::instrument(level = "trace", skip(self, path))]
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        let call_range = self.call_range.take();
        self.uses.paths.push(SourcePathUse {
            range: path.span().byte_range().into(),
            segments: path
                .path
                .segments
                .iter()
                .map(|segment| {
                    segment
                        .ident
                        .to_string()
                        .trim_start_matches("r#")
                        .to_owned()
                })
                .collect(),
            absolute: path.path.leading_colon.is_some(),
            qualified: path.qself.is_some()
                || path
                    .path
                    .segments
                    .iter()
                    .any(|segment| !matches!(segment.arguments, syn::PathArguments::None)),
            owner_signature: self.owner_signature.clone(),
            call_range: call_range.clone(),
            nested_scope: self.nested_scope,
        });
        syn::visit::visit_expr_path(self, path);
        self.call_range = call_range;
    }

    #[tracing::instrument(level = "trace", skip(self, call))]
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        let previous = self.call_range.take();
        if matches!(call.func.as_ref(), syn::Expr::Path(_)) {
            self.call_range = Some(call.span().byte_range().into());
        }
        self.visit_expr(&call.func);
        self.call_range = None;
        for argument in &call.args {
            self.visit_expr(argument);
        }
        self.call_range = previous;
    }

    #[tracing::instrument(level = "trace", skip(self, expression))]
    fn visit_expr(&mut self, expression: &'ast syn::Expr) {
        let previous = self.nested_scope;
        self.nested_scope |= matches!(
            expression,
            syn::Expr::Closure(_) | syn::Expr::Async(_) | syn::Expr::Const(_)
        );
        if let syn::Expr::Verbatim(tokens) = expression {
            self.uses
                .opaque_ranges
                .push(tokens.span().byte_range().into());
        }
        syn::visit::visit_expr(self, expression);
        self.nested_scope = previous;
    }

    #[tracing::instrument(level = "trace", skip(self, syntax))]
    fn visit_macro(&mut self, syntax: &'ast syn::Macro) {
        self.uses
            .opaque_ranges
            .push(syntax.span().byte_range().into());
        syn::visit::visit_macro(self, syntax);
    }

    #[tracing::instrument(level = "trace", skip(self, syntax))]
    fn visit_type(&mut self, syntax: &'ast syn::Type) {
        if let syn::Type::Verbatim(tokens) = syntax {
            self.uses
                .opaque_ranges
                .push(tokens.span().byte_range().into());
        }
        syn::visit::visit_type(self, syntax);
    }

    #[tracing::instrument(level = "trace", skip(self, syntax))]
    fn visit_pat(&mut self, syntax: &'ast syn::Pat) {
        if let syn::Pat::Verbatim(tokens) = syntax {
            self.uses
                .opaque_ranges
                .push(tokens.span().byte_range().into());
        }
        syn::visit::visit_pat(self, syntax);
    }
}
