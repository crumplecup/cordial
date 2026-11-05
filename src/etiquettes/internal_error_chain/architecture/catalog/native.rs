//! Architecture facts collected only from a loader-issued canonical tree.
use super::{Catalog, FanoutCall, FanoutWrapper, MapErrSite};
use crate::loader::{DatabaseSource, module_path_from_src_file};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasName},
};
use std::{collections::BTreeMap, path::Path};

mod aliases;
mod constructors;
mod declarations;
mod fanout;
pub(super) mod syntax;

struct Collector<'a, 'db> {
    file: &'a DatabaseSource<'db>,
    catalog: &'a mut Catalog,
    prefix: Vec<String>,
    aliases: BTreeMap<String, ast::Type>,
    macros: crate::loader::DatabaseMacroSyntax,
}

impl Catalog {
    #[tracing::instrument(level = "debug", skip(self, file))]
    #[hotpath::measure(impl_type = "ErrorArchitectureCatalog")]
    pub(crate) fn collect_native(&mut self, file: &DatabaseSource<'_>, root: &Path) {
        let mut collector = Collector {
            file,
            catalog: self,
            prefix: module_path_from_src_file(root, file.path()),
            aliases: aliases::collect(file.tree().syntax()),
            macros: file.proof_macro_syntax(),
        };
        collector.walk(file.tree().syntax(), false);
    }
}

impl Collector<'_, '_> {
    #[tracing::instrument(level = "trace", skip(self))]
    fn qualified(&self, name: &str) -> String {
        self.prefix
            .iter()
            .map(String::as_str)
            .chain([name])
            .collect::<Vec<_>>()
            .join("::")
    }

    #[tracing::instrument(level = "trace", skip(self, node))]
    fn walk(&mut self, node: &SyntaxNode, inside_kind: bool) {
        if ast::MacroCall::can_cast(node.kind()) || ast::Attr::can_cast(node.kind()) {
            return;
        }
        if let Some(module) = ast::Module::cast(node.clone()) {
            if syntax::cfg_test(&module) {
                return;
            }
            if let Some(list) = module.item_list() {
                let old = std::mem::replace(&mut self.aliases, aliases::collect(list.syntax()));
                if let Some(name) = module.name() {
                    self.prefix.push(name.text().to_string());
                    self.walk(list.syntax(), inside_kind);
                    self.prefix.pop();
                }
                self.aliases = old;
            }
            return;
        }
        if let Some(function) = ast::Fn::cast(node.clone())
            && !node
                .parent()
                .is_some_and(|p| ast::AssocItemList::can_cast(p.kind()))
        {
            if syntax::cfg_test(&function) {
                return;
            }
            if let (Some(target), Some(name)) = (fanout::wrapper(&function), function.name()) {
                self.catalog.fanout_wrappers.push(FanoutWrapper::new(
                    self.file.path().to_path_buf(),
                    None,
                    name.text().to_string(),
                    self.file.line(name.syntax().text_range().start()),
                    target,
                ));
            }
        }
        if let Some(item) = ast::Struct::cast(node.clone()) {
            self.structure(&item);
        }
        if let Some(item) = ast::Enum::cast(node.clone()) {
            self.enumeration(&item);
        }
        if let Some(item) = ast::Impl::cast(node.clone()) {
            self.implementation(&item);
        }
        let mut nested_kind = inside_kind;
        if let Some(call) = ast::CallExpr::cast(node.clone()) {
            let direct = fanout::direct(&call, &self.macros, true);
            nested_kind |= direct.is_some();
            let target = direct.or_else(|| {
                (!inside_kind)
                    .then(|| fanout::direct(&call, &self.macros, false))
                    .flatten()
            });
            if let Some((target, literal)) = target {
                self.catalog.fanout_calls.push(FanoutCall::new(
                    self.file.path().to_path_buf(),
                    self.file.line(node.text_range().start()),
                    None,
                    String::new(),
                    Some(target),
                    literal,
                ));
            } else if let Some(arg) = syntax::one_arg(&call)
                && let Some((owner, name)) = call.expr().and_then(|e| syntax::callee(&e))
                && let Some(literal) = fanout::literal(&arg, &self.macros)
            {
                self.catalog.fanout_calls.push(FanoutCall::new(
                    self.file.path().to_path_buf(),
                    self.file.line(node.text_range().start()),
                    owner,
                    name,
                    None,
                    literal,
                ));
            }
        }
        if let Some(call) = ast::MethodCallExpr::cast(node.clone())
            && let Some(name) = call.name_ref().filter(|n| n.text() == "map_err")
            && let Some(ast::Expr::PathExpr(path)) = call.arg_list().and_then(|a| a.args().next())
            && let Some((Some(owner), method)) = path.path().map(|p| syntax::path_pair(&p))
        {
            self.catalog.map_err_sites.push(MapErrSite {
                type_ident: owner,
                method,
                file: self.file.path().to_path_buf(),
                line: self.file.line(name.syntax().text_range().start()),
            });
        }
        for child in node.children() {
            self.walk(&child, nested_kind);
        }
    }
}
