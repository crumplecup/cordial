//! Constructor contracts and location capture from canonical bodies.
use super::{Collector, fanout, syntax};
use super::{ConstructorRec, FanoutWrapper};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasName},
};

impl Collector<'_, '_> {
    #[tracing::instrument(level = "trace", skip(self, item))]
    pub(super) fn implementation(&mut self, item: &ast::Impl) {
        if syntax::cfg_test(item) {
            return;
        }
        let Some(ty) = item.self_ty() else { return };
        let label = syntax::type_label(&ty);
        let ident = label.rsplit("::").next().unwrap_or(&label).to_string();
        let trait_name = item
            .trait_()
            .map(|t| syntax::type_label(&t))
            .and_then(|l| l.rsplit("::").next().map(str::to_string));
        if trait_name.as_deref() == Some("Copy") {
            self.catalog.copy_impls.insert(ident.clone());
        }
        if trait_name.as_deref() == Some("Error") {
            self.catalog.error_impls.insert(ident);
            return;
        }
        if trait_name.as_deref().is_some_and(|t| t != "From") {
            return;
        }
        let from = trait_name.is_some();
        for method in item
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
        {
            let ast::AssocItem::Fn(method) = method else {
                continue;
            };
            let Some(name) = method.name() else { continue };
            if from && name.text() != "from" {
                continue;
            }
            if !from {
                if let Some(target) = fanout::wrapper(&method) {
                    self.catalog.fanout_wrappers.push(FanoutWrapper::new(
                        self.file.path().to_path_buf(),
                        Some(ident.clone()),
                        name.text().to_string(),
                        self.file.line(name.syntax().text_range().start()),
                        target,
                    ));
                }
                if method
                    .param_list()
                    .is_some_and(|p| p.self_param().is_some())
                {
                    continue;
                }
                let Some(output) = method.ret_type().and_then(|t| t.ty()) else {
                    continue;
                };
                let output = syntax::type_label(&output);
                if output != "Self" && output != ident && !output.ends_with(&format!("::{ident}")) {
                    continue;
                }
            }
            let params: Vec<_> = method
                .param_list()
                .into_iter()
                .flat_map(|p| p.params())
                .collect();
            self.catalog.constructors.push(ConstructorRec {
                self_ident: ident.clone(),
                name: if from {
                    "From::from".into()
                } else {
                    name.text().to_string()
                },
                line: self.file.line(method.syntax().text_range().start()),
                has_track_caller: method
                    .attrs()
                    .any(|a| a.simple_name().as_deref() == Some("track_caller")),
                instrument_ret: self.instrument_ret(&method),
                captures_location: captures_location(&method),
                from_trait: from,
                input_labels: params
                    .iter()
                    .filter_map(|p| p.ty())
                    .map(|t| syntax::type_label(&t))
                    .collect(),
                takes_location_arg: params.iter().any(takes_location),
            });
        }
    }
}
#[tracing::instrument(level = "trace", skip(param))]
fn takes_location(param: &ast::Param) -> bool {
    if param
        .ty()
        .is_some_and(|ty| syntax::type_label(&ty).rsplit("::").next() == Some("Location"))
    {
        return true;
    }
    let Some(ast::Pat::IdentPat(pattern)) = param.pat() else {
        return false;
    };
    pattern
        .name()
        .is_some_and(|name| matches!(name.text(), "file" | "line" | "location"))
}
#[tracing::instrument(level = "trace", skip(function))]
fn captures_location(function: &ast::Fn) -> bool {
    let Some(body) = function.body() else {
        return false;
    };
    body.syntax().descendants().any(|node| {
        if node.ancestors().any(|n| ast::TokenTree::can_cast(n.kind())) {
            return false;
        }
        if let Some(path) = ast::PathExpr::cast(node.clone()).and_then(|p| p.path()) {
            let label = syntax::path_label(&path);
            let parts: Vec<_> = label.split("::").collect();
            if parts.windows(2).any(|p| p == ["Location", "caller"]) {
                return true;
            }
        }
        ast::MethodCallExpr::cast(node)
            .and_then(|m| m.name_ref())
            .is_some_and(|n| n.text() == "caller")
    })
}
impl Collector<'_, '_> {
    #[tracing::instrument(level = "trace", skip(self, function))]
    fn instrument_ret(&self, function: &ast::Fn) -> bool {
        function.attrs().any(|a| {
            if a.path()
                .is_none_or(|p| syntax::path_label(&p).rsplit("::").next() != Some("instrument"))
            {
                return false;
            }
            // The shared attribute grammar validates every argument and retains its path.
            self.file.meta_has_argument(&a, "ret")
        })
    }
}
