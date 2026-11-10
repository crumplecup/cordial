//! Collected Error-implementing types for architecture lints.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::super::type_graph::last_ident;

use tracing::instrument;

use crate::loader::{DatabaseSource, module_path_from_src_file};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasName},
};
use std::path::Path;

mod aliases;
mod constructors;
mod declarations;
mod fanout;
mod fields;
mod syntax;

pub(super) use fanout::{FanoutCall, FanoutTarget, FanoutWrapper};
use fields::FieldType;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct StructInfo {
    ident: String,
    type_path: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    snippet: String,
    kind_box_of: Option<String>,
    kind_borrowed_of: Option<String>,
    kind_unboxed_of: Option<String>,
    foreign_source: Option<String>,
    #[getter(copy)]
    has_source_field: bool,
    #[getter(copy)]
    has_file: bool,
    #[getter(copy)]
    has_line: bool,
    #[getter(copy)]
    has_location: bool,
}

impl StructInfo {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn location_complete(&self) -> bool {
        self.has_file && self.has_line
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct VariantInfo {
    name: String,
    #[getter(copy)]
    line: u32,
    snippet: String,
    payloads: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct EnumInfo {
    ident: String,
    type_path: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    snippet: String,
    variants: Vec<VariantInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct ConstructorRec {
    self_ident: String,
    name: String,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    has_track_caller: bool,
    #[getter(copy)]
    instrument_ret: bool,
    #[getter(copy)]
    captures_location: bool,
    #[getter(copy)]
    from_trait: bool,
    input_labels: Vec<String>,
    #[getter(copy)]
    takes_location_arg: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct MapErrSite {
    type_ident: String,
    method: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct ErrorField {
    owner: String,
    name: String,
    context: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    ty: FieldType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct Catalog {
    crate_name: String,
    structs: BTreeMap<String, StructInfo>,
    enums: BTreeMap<String, EnumInfo>,
    constructors: Vec<ConstructorRec>,
    map_err_sites: Vec<MapErrSite>,
    fanout_wrappers: Vec<FanoutWrapper>,
    fanout_calls: Vec<FanoutCall>,
    error_impls: BTreeSet<String>,
    fields: Vec<ErrorField>,
    copy_impls: BTreeSet<String>,
}

impl Catalog {
    #[instrument(level = "debug", fields(crate_name = crate_name))]
    pub(crate) fn new(crate_name: &str) -> Self {
        Self {
            crate_name: crate_name.to_string(),
            structs: BTreeMap::new(),
            enums: BTreeMap::new(),
            constructors: Vec::new(),
            map_err_sites: Vec::new(),
            fanout_wrappers: Vec::new(),
            fanout_calls: Vec::new(),
            error_impls: BTreeSet::new(),
            fields: Vec::new(),
            copy_impls: BTreeSet::new(),
        }
    }

    #[instrument(level = "debug")]
    pub(super) fn last_ident(label: &str) -> &str {
        last_ident(label)
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn impls_error(&self, ident: &str) -> bool {
        self.error_impls.contains(ident)
    }

    #[instrument(level = "trace", ret)]
    pub(super) fn is_kind_name(ident: &str) -> bool {
        ident.ends_with("Kind")
    }

    #[instrument(level = "trace", ret)]
    pub(super) fn is_error_enum_name(ident: &str) -> bool {
        ident.ends_with("Error") && !Self::is_kind_name(ident)
    }

    #[instrument(level = "trace", skip(self, item))]
    pub(super) fn is_error_kind(&self, item: &EnumInfo) -> bool {
        let boxed_by_error = self.structs.values().any(|item_struct| {
            self.impls_error(&item_struct.ident)
                && (item_struct.kind_box_of.as_deref() == Some(item.ident.as_str())
                    || item_struct.kind_borrowed_of.as_deref() == Some(item.ident.as_str())
                    || item_struct.kind_unboxed_of.as_deref() == Some(item.ident.as_str()))
        });
        if boxed_by_error {
            return true;
        }
        Self::is_kind_name(&item.ident)
            && item.variants.iter().any(|variant| {
                variant
                    .payloads
                    .iter()
                    .any(|payload| self.impls_error(Self::last_ident(payload)))
            })
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn kind_payload_idents(&self) -> BTreeSet<String> {
        let mut idents = BTreeSet::new();
        for item in self.enums.values() {
            if !self.is_error_kind(item) {
                continue;
            }
            for variant in &item.variants {
                for payload in &variant.payloads {
                    idents.insert(Self::last_ident(payload).to_string());
                }
            }
        }
        idents
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn root_parents(&self) -> Vec<&StructInfo> {
        let payloads = self.kind_payload_idents();
        self.structs
            .values()
            .filter(|item| {
                self.impls_error(&item.ident)
                    && (item.kind_box_of.is_some() || item.kind_borrowed_of.is_some())
                    && !payloads.contains(&item.ident)
            })
            .collect()
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn native_source_idents(&self) -> BTreeSet<String> {
        let parents: BTreeSet<String> = self
            .root_parents()
            .into_iter()
            .map(|item| item.ident.clone())
            .collect();
        self.error_impls
            .iter()
            .filter(|ident| self.structs.contains_key(*ident) && !parents.contains(*ident))
            .cloned()
            .collect()
    }
}

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
    pub(crate) fn collect(&mut self, file: &DatabaseSource<'_>, root: &Path) {
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
