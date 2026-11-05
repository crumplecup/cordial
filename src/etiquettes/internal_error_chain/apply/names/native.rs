//! Name-safety policy collected from the canonical database tree.

use super::{NameAudit, protected_name};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};

impl NameAudit {
    #[tracing::instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "NameAudit")]
    pub(crate) fn collect_native(file: &DatabaseSource<'_>) -> Self {
        let mut visitor = NameAudit::default();
        for node in file.tree().syntax().descendants().filter(|node| {
            !node
                .ancestors()
                .skip(1)
                .any(|parent| ast::Module::can_cast(parent.kind()))
        }) {
            if let Some(parameter) = ast::TypeParam::cast(node.clone()) {
                visitor.ambiguous |= parameter
                    .name()
                    .is_some_and(|name| protected_name(&name.text()));
            }
            // syn distinguishes free items from associated and foreign items.
            if node.parent().is_some_and(|parent| {
                ast::AssocItemList::can_cast(parent.kind())
                    || ast::ExternItemList::can_cast(parent.kind())
            }) {
                continue;
            }
            let Some(item) = ast::Item::cast(node) else {
                continue;
            };
            visitor.inspect_native_item(item);
        }
        visitor
    }
}

impl NameAudit {
    #[tracing::instrument(level = "trace", skip(self, item))]
    fn inspect_native_item(&mut self, item: ast::Item) {
        let name = match item {
            ast::Item::Struct(item) => item.name(),
            ast::Item::Enum(item) => item.name(),
            ast::Item::Union(item) => item.name(),
            ast::Item::Trait(item) => item.name(),
            ast::Item::Module(item) => item.name(),
            ast::Item::Const(item) => item.name(),
            ast::Item::Static(item) => item.name(),
            ast::Item::Fn(item) => item.name(),
            ast::Item::MacroRules(item) => item.name(),
            ast::Item::ExternCrate(item) => {
                let name = item
                    .rename()
                    .and_then(|rename| {
                        rename
                            .name()
                            .map(|name| name.text().to_string())
                            .or_else(|| rename.underscore_token().map(|_| "_".to_string()))
                    })
                    .or_else(|| item.name_ref().map(|name| name.text().to_string()));
                self.ambiguous |= name.is_some_and(|name| protected_name(&name));
                return;
            }
            ast::Item::TypeAlias(item) => {
                if let (Some(name), Some(ast::Type::PathType(ty))) = (item.name(), item.ty()) {
                    if let Some(path) = ty.path().filter(|path| !has_type_anchor(path)) {
                        if let Some(target) = path.segment().and_then(|segment| segment.name_ref())
                        {
                            if name.text() != target.text() {
                                self.aliased_types.insert(target.text().to_string());
                            }
                        }
                    }
                }
                item.name()
            }
            ast::Item::Use(item) => {
                if let Some(tree) = item.use_tree() {
                    self.inspect_native_use(&tree, Vec::new());
                }
                return;
            }
            _ => None,
        };
        self.ambiguous |= name.is_some_and(|name| protected_name(&name.text()));
    }

    #[tracing::instrument(level = "trace", skip(self, tree, prefix))]
    fn inspect_native_use(&mut self, tree: &ast::UseTree, mut prefix: Vec<String>) {
        if tree.star_token().is_some() {
            self.ambiguous = true;
            return;
        }
        let mut parts = Vec::new();
        let mut current = tree.path();
        while let Some(path) = current {
            if let Some(name) = path.segment().and_then(|part| part.name_ref()) {
                parts.push(name.text().to_string());
            }
            current = path.qualifier();
        }
        prefix.extend(parts.into_iter().rev());
        if let Some(group) = tree.use_tree_list() {
            for child in group.use_trees() {
                self.inspect_native_use(&child, prefix.clone());
            }
        } else if let Some(name) = tree
            .rename()
            .and_then(|rename| {
                rename
                    .name()
                    .map(|name| name.text().to_string())
                    .or_else(|| rename.underscore_token().map(|_| "_".to_string()))
            })
            .or_else(|| prefix.last().cloned())
        {
            self.check_binding(&name, &prefix);
        }
    }
}

#[tracing::instrument(level = "trace", skip(path))]
fn has_type_anchor(path: &ast::Path) -> bool {
    let mut current = Some(path.clone());
    while let Some(path) = current {
        if path
            .segment()
            .is_some_and(|segment| segment.type_anchor().is_some())
        {
            return true;
        }
        current = path.qualifier();
    }
    false
}
