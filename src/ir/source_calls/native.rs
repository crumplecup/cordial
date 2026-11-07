//! Namespace and written-use inventory from the canonical syntax carrier.

use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasGenericArgs},
};

use super::{SourceModuleScope, SourcePathUse, SourceUseInventory};
use crate::loader::DatabaseSource;

impl SourceModuleScope {
    #[tracing::instrument(level = "trace", skip(file))]
    #[hotpath::measure(impl_type = "SourceModuleScope")]
    pub(crate) fn collect_native(file: &DatabaseSource<'_>) -> Self {
        let mut scope = Self {
            source_digest: file.source_digest(),
            insertion_offset: file.source().len(),
            identifiers: Vec::new(),
            opaque: false,
            extern_crates: false,
            uses: None,
        };
        scope.collect_nodes(file);
        scope.identifiers = collect_identifiers(file);
        scope
    }

    #[tracing::instrument(level = "trace", skip(self, file))]
    fn collect_nodes(&mut self, file: &DatabaseSource<'_>) {
        let mut uses = SourceUseInventory {
            paths: Vec::new(),
            opaque_ranges: Vec::new(),
        };
        for node in file.tree().syntax().descendants() {
            self.extern_crates |= ast::ExternCrate::can_cast(node.kind());
            self.opaque |= matches!(
                node.kind(),
                SyntaxKind::USE
                    | SyntaxKind::EXTERN_CRATE
                    | SyntaxKind::MACRO_RULES
                    | SyntaxKind::MACRO_DEF
                    | SyntaxKind::ERROR
            );
            self.opaque |= ast::MacroCall::can_cast(node.kind())
                && !node
                    .parent()
                    .is_some_and(|parent| ast::MacroExpr::can_cast(parent.kind()));
            if let Some(attr) = ast::Attr::cast(node.clone()) {
                self.opaque |= !attr.simple_name().is_some_and(|name| name == "doc");
            }
            if matches!(
                node.kind(),
                SyntaxKind::MACRO_CALL
                    | SyntaxKind::MACRO_RULES
                    | SyntaxKind::MACRO_DEF
                    | SyntaxKind::ERROR
            ) {
                uses.opaque_ranges
                    .push(std::ops::Range::<usize>::from(node.text_range()).into());
            }
            if let Some(expression) = ast::PathExpr::cast(node.clone())
                && let Some(path) = expression.path()
            {
                uses.paths.push(path_use(&expression, path));
            }
        }
        self.uses = Some(uses);
    }
}

#[tracing::instrument(level = "trace", skip(file))]
fn collect_identifiers(file: &DatabaseSource<'_>) -> Vec<String> {
    let mut identifiers = Vec::new();
    for token in file
        .tree()
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        if !matches!(
            token.kind(),
            SyntaxKind::IDENT
                | SyntaxKind::LIFETIME_IDENT
                | SyntaxKind::SELF_KW
                | SyntaxKind::SELF_TYPE_KW
                | SyntaxKind::CRATE_KW
                | SyntaxKind::SUPER_KW
        ) {
            continue;
        }
        if token
            .parent_ancestors()
            .any(|node| matches!(node.kind(), SyntaxKind::ATTR | SyntaxKind::TOKEN_TREE))
        {
            continue;
        }
        // A receiver's `self` is a language token rather than a visited Ident
        // in the compatibility contract. Written paths such as `self::item`
        // and value expressions still contribute their ordinary identifiers.
        if token.kind() == SyntaxKind::SELF_KW
            && token
                .parent_ancestors()
                .any(|node| ast::SelfParam::can_cast(node.kind()))
        {
            continue;
        }
        identifiers.push(
            token
                .text()
                .trim_start_matches('\'')
                .trim_start_matches("r#")
                .to_owned(),
        );
    }
    identifiers.sort();
    identifiers.dedup();
    identifiers
}

#[tracing::instrument(level = "trace", skip(expression, path))]
fn path_use(expression: &ast::PathExpr, path: ast::Path) -> SourcePathUse {
    let mut current = path;
    let mut segments = Vec::new();
    let mut qualified = false;
    let absolute;
    loop {
        if let Some(segment) = current.segment() {
            qualified |= segment.generic_arg_list().is_some() || segment.type_anchor().is_some();
            if let Some(name) = segment.name_ref() {
                segments.push(name.text().trim_start_matches("r#").to_owned());
            }
        }
        match current.qualifier() {
            Some(qualifier) => current = qualifier,
            None => {
                absolute = current
                    .segment()
                    .is_some_and(|segment| segment.coloncolon_token().is_some());
                break;
            }
        }
    }
    segments.reverse();
    let mut owner_signature = None;
    let mut nested_scope = false;
    let mut item_count = 0;
    for ancestor in expression.syntax().ancestors() {
        // Associated items are members of the enclosing impl/trait, rather
        // than another nested Item scope in the compatibility contract.
        if ast::Item::can_cast(ancestor.kind())
            && !ancestor
                .parent()
                .is_some_and(|parent| ast::AssocItemList::can_cast(parent.kind()))
        {
            item_count += 1;
            if ancestor
                .parent()
                .is_some_and(|parent| ast::SourceFile::can_cast(parent.kind()))
            {
                owner_signature = ast::Fn::cast(ancestor.clone())
                    .and_then(|function| crate::ir::native_signature_range(&function))
                    .map(|range| std::ops::Range::<usize>::from(range).into());
            }
        }
        nested_scope |= ast::ClosureExpr::can_cast(ancestor.kind())
            || ast::BlockExpr::cast(ancestor).is_some_and(|block| {
                block.async_token().is_some() || block.const_token().is_some()
            });
    }
    nested_scope |= item_count > 1;
    let call_range = expression
        .syntax()
        .parent()
        .and_then(ast::CallExpr::cast)
        .filter(|call| call.expr().as_ref().map(AstNode::syntax) == Some(expression.syntax()))
        .map(|call| std::ops::Range::<usize>::from(call.syntax().text_range()).into());
    SourcePathUse {
        range: std::ops::Range::<usize>::from(expression.syntax().text_range()).into(),
        segments,
        absolute,
        qualified,
        owner_signature,
        call_range,
        nested_scope,
    }
}
