//! Located written uses for conservative error-graph closure checks.
//!
//! Collection is independent of the candidate graph. Consumers select tracked
//! names from this required snapshot evidence without traversing syntax again.

use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode, SyntaxKind, TextSize,
    ast::{self, HasName},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod paths;
mod tokens;

pub(crate) type LocatedName = (String, usize, usize);

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct ApplyUses {
    source_identifiers: BTreeSet<String>,
    identifiers: BTreeSet<LocatedName>,
    declarations: BTreeSet<LocatedName>,
    paths: Vec<(LocatedName, Vec<String>)>,
    macro_names: BTreeSet<String>,
    #[getter(copy)]
    has_cfg: bool,
}

impl ApplyUses {
    #[tracing::instrument(level = "trace", skip(source))]
    fn empty(source: &str) -> Self {
        Self {
            // Intentionally includes comments and literals: textual mentions
            // are a separate, conservative guard from syntactic uses.
            source_identifiers: source
                .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
                .map(str::to_string)
                .collect(),
            identifiers: BTreeSet::new(),
            declarations: BTreeSet::new(),
            paths: Vec::new(),
            macro_names: BTreeSet::new(),
            has_cfg: false,
        }
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn normalize(&mut self) {
        // Graph closure tests membership, not traversal order. Qualified-self
        // paths may expose the same prefix through several AST nodes.
        self.paths.sort();
        self.paths.dedup();
    }
}

impl ApplyUses {
    #[tracing::instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "ApplyUses")]
    pub(in crate::ir::error_apply_source) fn collect_native(file: &DatabaseSource<'_>) -> Self {
        let mut facts = Self::empty(file.source());
        for node in file.tree().syntax().descendants() {
            let declaration = if let Some(item) = ast::Struct::cast(node.clone()) {
                item.name()
            } else {
                ast::Enum::cast(node.clone()).and_then(|item| item.name())
            };
            if let Some(name) = declaration {
                facts.declarations.insert(located(
                    file,
                    name.text().to_string(),
                    name.syntax().text_range().start(),
                ));
            }
            if let Some(attr) = ast::Attr::cast(node.clone()) {
                facts.has_cfg |= !matches!(attr.meta(), Some(ast::Meta::UnsafeMeta(_)))
                    && attr
                        .simple_name()
                        .is_some_and(|name| matches!(name.as_str(), "cfg" | "cfg_attr"))
                    && attr
                        .path()
                        .is_none_or(|path| path.coloncolon_token().is_none());
                let keyword = match attr.meta() {
                    Some(ast::Meta::CfgMeta(meta)) => meta.cfg_token(),
                    Some(ast::Meta::CfgAttrMeta(meta)) => meta.cfg_attr_token(),
                    Some(ast::Meta::UnsafeMeta(meta)) => meta.unsafe_token(),
                    _ => None,
                };
                if let Some(token) = keyword {
                    let name = located(file, token.text().to_string(), token.text_range().start());
                    facts.identifiers.insert(name.clone());
                    if !node
                        .ancestors()
                        .any(|node| ast::Module::can_cast(node.kind()))
                    {
                        facts.paths.push((name, vec![token.text().to_string()]));
                    }
                }
            }
        }
        facts.collect_tokens(file);
        paths::collect(file, &mut facts.paths);
        facts.normalize();
        facts
    }
}

#[tracing::instrument(level = "trace", skip(file, offset))]
fn located(file: &DatabaseSource<'_>, name: String, offset: TextSize) -> LocatedName {
    let offset_bytes = usize::from(offset);
    let start = file.source()[..offset_bytes]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    (
        name,
        file.line(offset) as usize,
        file.source()[start..offset_bytes].chars().count(),
    )
}

#[tracing::instrument(level = "trace", skip(kind))]
fn opaque_meta(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CFG_META | SyntaxKind::CFG_ATTR_META | SyntaxKind::UNSAFE_META
    )
}
