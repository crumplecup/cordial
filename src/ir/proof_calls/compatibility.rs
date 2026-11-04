//! Shared-syn compatibility projection at the loader boundary.

use super::{SourceProofFunction, assert_arguments, qualify};
use crate::etiquettes::tracing::{self_type_key, syn_path_label, type_label};
use crate::loader::{SharedMacroSyntax, SharedParsedFile};
use syn::visit::Visit;

impl SourceProofFunction {
    #[tracing::instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "SourceProofFunction")]
    pub(crate) fn collect(file: &SharedParsedFile, prefix: &[String]) -> Vec<Self> {
        let mut out = Vec::new();
        items(file, &file.syntax().items, prefix, &[], &mut out);
        out
    }
}

#[tracing::instrument(level = "debug", skip(file, source, out))]
fn items(
    file: &SharedParsedFile,
    source: &[syn::Item],
    prefix: &[String],
    inherited_cfgs: &[String],
    out: &mut Vec<SourceProofFunction>,
) {
    for item in source {
        match item {
            syn::Item::Fn(function) => {
                let name = function.sig.ident.to_string();
                out.push(function_facts(
                    file,
                    prefix,
                    &name,
                    vec![name.clone()],
                    (inherited_cfgs, &[]),
                    &function.attrs,
                    &function.block,
                ));
            }
            syn::Item::Mod(module) => {
                if let Some((_, nested)) = &module.content {
                    let mut prefix = prefix.to_vec();
                    prefix.push(module.ident.to_string());
                    let mut cfgs = inherited_cfgs.to_vec();
                    cfgs.extend(cfg_names(&module.attrs));
                    items(file, nested, &prefix, &cfgs, out);
                }
            }
            syn::Item::Impl(implementation) => {
                let bare = type_label(&implementation.self_ty);
                let key = self_type_key(&implementation.self_ty);
                let trait_name = implementation
                    .trait_
                    .as_ref()
                    .map(|(path, _)| syn_path_label(path));
                let mut cfgs = inherited_cfgs.to_vec();
                cfgs.extend(cfg_names(&implementation.attrs));
                let attrs = attribute_names(&implementation.attrs);
                for method in &implementation.items {
                    if let syn::ImplItem::Fn(method) = method {
                        let name = method.sig.ident.to_string();
                        let local = match &trait_name {
                            Some(trait_name) => format!("<{key} as {trait_name}>::{name}"),
                            None => format!("{key}::{name}"),
                        };
                        let mut keys = vec![format!("{bare}::{name}")];
                        if let Some(trait_name) = &trait_name {
                            keys.push(format!("{trait_name}::{name}"));
                        }
                        out.push(function_facts(
                            file,
                            prefix,
                            &local,
                            keys,
                            (&cfgs, &attrs),
                            &method.attrs,
                            &method.block,
                        ));
                    }
                }
            }
            syn::Item::Macro(invocation) => {
                if let Some(
                    SharedMacroSyntax::Items(nested) | SharedMacroSyntax::TrailingItems(nested),
                ) = file.macro_syntax(&invocation.mac)
                {
                    items(file, nested, prefix, inherited_cfgs, out);
                }
            }
            _ => {}
        }
    }
}

#[tracing::instrument(level = "debug", skip(file, attrs, body))]
fn function_facts(
    file: &SharedParsedFile,
    prefix: &[String],
    local: &str,
    call_keys: Vec<String>,
    inherited: (&[String], &[String]),
    attrs: &[syn::Attribute],
    body: &syn::Block,
) -> SourceProofFunction {
    let mut cfgs = inherited.0.to_vec();
    cfgs.extend(cfg_names(attrs));
    let mut names = inherited.1.to_vec();
    names.extend(attribute_names(attrs));
    let mut visitor = Calls {
        file,
        calls: Vec::new(),
    };
    visitor.visit_block(body);
    SourceProofFunction {
        qualified_name: qualify(prefix, local),
        call_keys,
        cfg_names: cfgs,
        attribute_names: names,
        calls: visitor.calls,
    }
}

#[tracing::instrument(level = "trace", skip(attrs))]
fn cfg_names(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut names = Vec::new();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("cfg")) {
        let _ = attr.parse_nested_meta(|meta| {
            if let Some(name) = meta.path.get_ident() {
                names.push(name.to_string());
            }
            Ok(())
        });
    }
    names
}

#[tracing::instrument(level = "trace", skip(attrs))]
fn attribute_names(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|attr| {
            attr.path()
                .segments
                .first()
                .map(|seg| seg.ident.to_string())
        })
        .collect()
}

struct Calls<'a> {
    file: &'a SharedParsedFile,
    calls: Vec<String>,
}

impl<'ast> Visit<'ast> for Calls<'_> {
    #[tracing::instrument(level = "trace", skip(self, call))]
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = call.func.as_ref() {
            let parts: Vec<_> = path
                .path
                .segments
                .iter()
                .rev()
                .take(2)
                .map(|seg| seg.ident.to_string())
                .collect();
            self.calls
                .push(parts.into_iter().rev().collect::<Vec<_>>().join("::"));
        }
        syn::visit::visit_expr_call(self, call);
    }

    #[tracing::instrument(level = "trace", skip(self, invocation))]
    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        let take = invocation
            .path
            .get_ident()
            .map(|name| assert_arguments(&name.to_string()))
            .unwrap_or(0);
        if take > 0
            && let Some(SharedMacroSyntax::Expressions(args)) = self.file.macro_syntax(invocation)
        {
            for arg in args.iter().take(take) {
                self.visit_expr(arg);
            }
        }
    }
}
