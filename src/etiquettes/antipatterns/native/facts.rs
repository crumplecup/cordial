//! Crate-wide relationships needed before policy-independent field collection.
use super::{CrateFacts, cfg_test, compact, types};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode, SyntaxNode, ast,
    ast::{HasAttrs, HasName},
};

impl CrateFacts {
    #[tracing::instrument(level = "debug", skip(self, source))]
    #[hotpath::measure]
    pub(super) fn collect(&mut self, source: &DatabaseSource<'_>) {
        // Error membership historically includes inactive/test declarations;
        // placement and local-trait exemptions exclude cfg(test) modules.
        for node in source.tree().syntax().descendants() {
            if let Some(implementation) = ast::Impl::cast(node.clone())
                && implementation
                    .trait_()
                    .as_ref()
                    .is_some_and(|ty| types::label(ty) == "Error")
                && let Some(ty) = implementation.self_ty()
                && let Some(name) = types::path(&ty).as_ref().and_then(types::path_name)
            {
                self.errors.insert(name);
            }
            if let Some(adt) = ast::Adt::cast(node.clone())
                && derives_error(&adt)
                && let Some(name) = adt.name()
            {
                self.errors.insert(name.text().to_string());
            }
        }
        self.placement(source.tree().syntax(), false);
    }

    /// Add the `kind` field types of every error struct among `pairs`, in order:
    /// a kind named by one struct can make the next struct an error.
    #[tracing::instrument(level = "debug", skip(self, pairs))]
    #[hotpath::measure]
    pub(super) fn collect_kinds(&mut self, pairs: &[(String, String)]) {
        for (structure, kind) in pairs {
            if self.errors.contains(structure) {
                self.errors.insert(kind.clone());
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self, node))]
    #[hotpath::measure]
    fn placement(&mut self, node: &SyntaxNode, in_const: bool) {
        if let Some(module) = ast::Module::cast(node.clone())
            && cfg_test(&module)
        {
            return;
        }
        if let Some(local) = ast::Trait::cast(node.clone()) {
            if let Some(name) = local.name() {
                self.traits.insert(name.text().to_string());
            }
            if let Some(list) = local.assoc_item_list() {
                for function in list.assoc_items().filter_map(|item| match item {
                    ast::AssocItem::Fn(f) => Some(f),
                    _ => None,
                }) {
                    if let Some(ast::Type::RefType(reference)) =
                        function.ret_type().and_then(|ret| ret.ty())
                        && reference
                            .lifetime()
                            .is_some_and(|l| compact(l.syntax()) == "'static")
                    {
                        let inner = match reference.ty() {
                            Some(ast::Type::SliceType(slice)) => slice.ty(),
                            other => other,
                        };
                        if let Some(name) = inner
                            .as_ref()
                            .and_then(types::path)
                            .as_ref()
                            .and_then(types::path_name)
                            && type_ident(&name)
                        {
                            self.constants.insert(name);
                        }
                    }
                }
            }
        }
        let in_const = if let Some(item) = ast::Const::cast(node.clone()) {
            if cfg_test(&item) {
                return;
            }
            true
        } else if let Some(item) = ast::Static::cast(node.clone()) {
            if cfg_test(&item) {
                return;
            }
            true
        } else {
            in_const
        };
        if let Some(call) = ast::MacroCall::cast(node.clone()) {
            // A path payload needs no second Rust parse: its identifier tokens
            // already belong to the database token tree.
            if call
                .path()
                .is_some_and(|path| compact(path.syntax()) == "inventory::collect")
                && let Some(tree) = call.token_tree()
                && let Some(name) = tree
                    .syntax()
                    .descendants_with_tokens()
                    .filter_map(|part| part.into_token())
                    .filter(|token| token.kind() == ra_ap_syntax::SyntaxKind::IDENT)
                    .last()
            {
                self.constants.insert(name.text().to_string());
            }
        }
        let path = if let Some(expr) = ast::RecordExpr::cast(node.clone()) {
            expr.path()
        } else if let Some(call) = ast::CallExpr::cast(node.clone()) {
            match call.expr() {
                Some(ast::Expr::PathExpr(path)) => path.path(),
                _ => None,
            }
        } else {
            None
        };
        if let Some(path) = path {
            let mut names = Vec::new();
            let mut path = Some(path);
            while let Some(current) = path {
                if let Some(name) = types::path_name(&current) {
                    names.push(name);
                }
                path = current.qualifier();
            }
            for name in names.into_iter().take(2).filter(|name| type_ident(name)) {
                if in_const {
                    self.constants.insert(name);
                } else {
                    self.runtime.insert(name);
                }
            }
        }
        for child in node.children() {
            self.placement(&child, in_const);
        }
    }
}

#[tracing::instrument(level = "trace")]
fn type_ident(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

#[tracing::instrument(level = "trace", skip(adt))]
fn derives_error(adt: &ast::Adt) -> bool {
    adt.attrs().any(|attr| {
        attr.path()
            .is_some_and(|path| compact(path.syntax()) == "derive")
            && attr
                .meta()
                .and_then(|meta| meta.syntax().children().find_map(ast::TokenTree::cast))
                .is_some_and(|tree| {
                    tree.syntax()
                        .descendants_with_tokens()
                        .filter_map(|part| part.into_token())
                        .any(|token| token.text() == "Error")
                })
    })
}

#[derive(serde::Serialize, serde::Deserialize)]
/// What one file contributes to the crate's facts: its own declarations and
/// uses, and every `kind` field it declares on a struct, whether or not that
/// struct turns out to be an error. Which structs are errors is crate-wide, so
/// the pairs wait for the join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileFacts {
    contribution: CrateFacts,
    kinds: Vec<(String, String)>,
}

impl FileFacts {
    #[tracing::instrument(level = "debug", skip(source))]
    #[hotpath::measure]
    pub(crate) fn collect(source: &DatabaseSource<'_>) -> Self {
        let mut contribution = CrateFacts::default();
        contribution.collect(source);
        Self {
            contribution,
            kinds: kind_pairs(source.tree().syntax()),
        }
    }
}

impl CrateFacts {
    /// The crate's facts from every file's contribution, in file order.
    #[tracing::instrument(level = "debug", skip(files))]
    #[hotpath::measure]
    pub(crate) fn join<'a>(files: impl Iterator<Item = &'a FileFacts> + Clone) -> Self {
        let mut facts = Self::default();
        for file in files.clone() {
            facts
                .traits
                .extend(file.contribution.traits.iter().cloned());
            facts
                .errors
                .extend(file.contribution.errors.iter().cloned());
            facts
                .constants
                .extend(file.contribution.constants.iter().cloned());
            facts
                .runtime
                .extend(file.contribution.runtime.iter().cloned());
        }
        for file in files {
            facts.collect_kinds(&file.kinds);
        }
        facts
    }
}

/// Each `kind` field of each named struct under `node`, as the struct's name and
/// the name of the type the field holds (through a reference or a `Box`) when it
/// ends in `Kind`.
#[tracing::instrument(level = "debug", skip(node))]
#[hotpath::measure]
fn kind_pairs(node: &SyntaxNode) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for structure in node.descendants().filter_map(ast::Struct::cast) {
        let Some(name) = structure.name() else {
            continue;
        };
        let Some(ast::FieldList::RecordFieldList(list)) = structure.field_list() else {
            continue;
        };
        for field in list
            .fields()
            .filter(|field| field.name().is_some_and(|name| name.text() == "kind"))
        {
            let Some(ty) = field.ty() else {
                continue;
            };
            let inner = match &ty {
                ast::Type::RefType(reference) => reference.ty(),
                _ if types::label(&ty) == "Box" => types::arguments(&ty).into_iter().next(),
                _ => Some(ty),
            };
            if let Some(kind) = inner
                .as_ref()
                .and_then(types::path)
                .as_ref()
                .and_then(types::path_name)
                && kind.ends_with("Kind")
            {
                pairs.push((name.text().to_string(), kind));
            }
        }
    }
    pairs
}
