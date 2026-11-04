//! Attribute projection from the database-issued canonical tree.

use super::{
    SourceAttribute, SourceAttributedItem, SourceCrateLints, SourceFileAttributes, native_types,
};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasAttrs, HasModuleItem, HasName},
};

use tracing::instrument;
impl SourceFileAttributes {
    #[instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "SourceFileAttributes")]
    pub(crate) fn collect_native(file: &DatabaseSource<'_>) -> Self {
        let mut items = Vec::new();
        collect_items(file, file.tree().items(), &[], &mut items);
        let mut lints = SourceCrateLints::default();
        for attr in file.tree().attrs() {
            if attr.excl_token().is_some() {
                if let Some(meta) = attr.meta() {
                    apply_meta(&meta, &mut lints);
                }
            }
        }
        Self {
            source_digest: file.source_digest(),
            insert_at: file
                .tree()
                .items()
                .next()
                .map(|item| file.line(item.syntax().text_range().start()) as usize - 1)
                .unwrap_or_else(|| file.source().lines().count()),
            lints,
            assume_specification: file.source().contains("assume_specification"),
            items,
        }
    }
}

#[instrument(level = "debug", skip(file, items, out))]
fn collect_items(
    file: &DatabaseSource<'_>,
    items: impl Iterator<Item = ast::Item>,
    prefix: &[String],
    out: &mut Vec<SourceAttributedItem>,
) {
    for item in items {
        match item {
            ast::Item::Fn(item) => {
                if let Some(name) = item.name() {
                    attach(file, &item, prefix, name.text(), out);
                }
            }
            ast::Item::Struct(item) => {
                if let Some(name) = item.name() {
                    attach(file, &item, prefix, name.text(), out);
                }
            }
            ast::Item::Enum(item) => {
                if let Some(name) = item.name() {
                    attach(file, &item, prefix, name.text(), out);
                }
            }
            ast::Item::Module(item) => {
                let test_only = item.attrs().any(|attr| {
                    let Some(ast::Meta::CfgMeta(meta)) = attr.meta() else {
                        return false;
                    };
                    meta.cfg_predicate()
                        .is_some_and(|predicate| significant_tokens(predicate.syntax()) == "test")
                });
                if test_only {
                    continue;
                }
                let Some(name) = item.name() else {
                    continue;
                };
                if let Some(list) = item.item_list() {
                    let mut nested = prefix.to_vec();
                    nested.push(name.text().to_string());
                    collect_items(file, list.items(), &nested, out);
                } else {
                    attach(file, &item, prefix, name.text(), out);
                }
            }
            ast::Item::Impl(item) => {
                let self_ty = item
                    .self_ty()
                    .map(|ty| native_types::type_key(&ty))
                    .unwrap_or_else(|| "?".into());
                let trait_name = item
                    .trait_()
                    .and_then(|ty| match ty {
                        ast::Type::PathType(ty) => ty.path(),
                        _ => None,
                    })
                    .and_then(|path| path.segment())
                    .and_then(|segment| segment.name_ref())
                    .map(|name| name.text().to_string());
                if let Some(list) = item.assoc_item_list() {
                    for method in list.assoc_items().filter_map(|item| match item {
                        ast::AssocItem::Fn(method) => Some(method),
                        _ => None,
                    }) {
                        let Some(name) = method.name() else {
                            continue;
                        };
                        let local = match &trait_name {
                            Some(trait_name) => format!("<{self_ty} as {trait_name}>::{name}"),
                            None => format!("{self_ty}::{name}"),
                        };
                        attach(file, &method, prefix, &local, out);
                    }
                }
            }
            _ => {}
        }
    }
}

#[instrument(level = "debug", skip(file, owner, out))]
fn attach(
    file: &DatabaseSource<'_>,
    owner: &dyn HasAttrs,
    prefix: &[String],
    local: &str,
    out: &mut Vec<SourceAttributedItem>,
) {
    let path = if prefix.is_empty() {
        local.into()
    } else {
        format!("{}::{local}", prefix.join("::"))
    };
    let attributes = ast::attrs_with_doc_including_inner(owner)
        .filter_map(|attr| match attr {
            ast::AnyAttr::DocComment(_) => Some(SourceAttribute {
                path: "doc".into(),
                meta: "doc = …".into(),
                range: None,
                instrument: false,
                instrument_syntax: None,
            }),
            ast::AnyAttr::Attr(attr) => attr.meta().map(|meta| {
                let mut projected = attribute(&meta);
                let range = attr.syntax().text_range();
                projected.range = Some(
                    (u32::from(range.start()) as usize..u32::from(range.end()) as usize).into(),
                );
                projected
            }),
        })
        .collect();
    out.push(SourceAttributedItem {
        path,
        range: std::ops::Range::<usize>::from(owner.syntax().text_range()).into(),
        line: file.line(owner.syntax().text_range().start()),
        attributes,
    });
}

#[instrument(level = "debug", skip(meta))]
fn attribute(meta: &ast::Meta) -> SourceAttribute {
    let path = if matches!(meta, ast::Meta::UnsafeMeta(_)) {
        "unsafe".into()
    } else {
        meta.path()
            .map(|path| significant_tokens(path.syntax()).replace(' ', ""))
            .or_else(|| meta.simple_name().map(|name| name.to_string()))
            .unwrap_or_default()
    };
    let path = path.trim_start_matches("::").to_string();
    let name = path.rsplit("::").next().unwrap_or(&path);
    let rendered = match meta {
        ast::Meta::PathMeta(_) => name.to_string(),
        ast::Meta::KeyValueMeta(_) => format!("{name} = …"),
        _ => {
            let tokens = token_parts(meta.syntax());
            let body: Vec<_> = tokens
                .into_iter()
                .skip_while(|token| token != "(")
                .skip(1)
                .collect();
            let body = &body[..body.len().saturating_sub(1)];
            format!("{name}({})", super::render_tokens(body.to_vec()))
        }
    };
    let instrument = instrument_path(&path)
        || match meta {
            ast::Meta::CfgAttrMeta(cfg) => {
                let metas: Vec<_> = cfg.metas().collect();
                metas.first().is_some_and(|inner| {
                    (metas.len() == 1 || matches!(inner, ast::Meta::TokenTreeMeta(_)))
                        && inner.path().is_some_and(|path| {
                            instrument_path(&significant_tokens(path.syntax()).replace(' ', ""))
                        })
                })
            }
            _ => false,
        };
    SourceAttribute {
        path,
        meta: rendered,
        range: None,
        instrument,
        instrument_syntax: match meta {
            ast::Meta::CfgAttrMeta(cfg) => {
                let metas: Vec<_> = cfg.metas().collect();
                if metas.len() == 1 {
                    super::instrument::collect(
                        &super::instrument::native_tokens(metas[0].syntax()),
                        cfg.cfg_predicate()
                            .map(|predicate| token_parts(predicate.syntax()).concat()),
                    )
                } else {
                    None
                }
            }
            _ => super::instrument::collect(&super::instrument::native_tokens(meta.syntax()), None),
        },
    }
}

#[instrument(level = "debug", skip(path))]
fn instrument_path(path: &str) -> bool {
    matches!(path, "instrument" | "tracing::instrument")
}

#[instrument(level = "debug", skip(node))]
fn token_parts(node: &SyntaxNode) -> Vec<String> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| token.text().to_string())
        .collect()
}

#[instrument(level = "debug", skip(node))]
fn significant_tokens(node: &SyntaxNode) -> String {
    super::render_tokens(token_parts(node))
}

#[instrument(level = "debug", skip(meta, presence))]
fn apply_meta(meta: &ast::Meta, presence: &mut SourceCrateLints) {
    if let ast::Meta::CfgAttrMeta(cfg) = meta {
        for inner in cfg.metas() {
            apply_level(&inner, presence);
        }
    } else {
        apply_level(meta, presence);
    }
}

#[instrument(level = "debug", skip(meta, presence))]
fn apply_level(meta: &ast::Meta, presence: &mut SourceCrateLints) {
    let ast::Meta::TokenTreeMeta(meta) = meta else {
        return;
    };
    let Some(path) = meta.path() else {
        return;
    };
    let level = significant_tokens(path.syntax());
    if !matches!(level.as_str(), "forbid" | "deny" | "warn") {
        return;
    }
    let Some(tree) = meta.token_tree() else {
        return;
    };
    // Inspect the upstream token-tree's immediate entries. Nested lint arguments
    // and string literals cannot introduce another lint name.
    let mut name = None;
    let mut in_path = true;
    for element in tree.syntax().children_with_tokens().skip(1) {
        match element.kind() {
            SyntaxKind::COMMA | SyntaxKind::R_PAREN => {
                match (level.as_str(), name.as_deref()) {
                    ("forbid", Some("unsafe_code")) => presence.forbid_unsafe_code = true,
                    (_, Some("missing_docs")) => presence.missing_docs_at_least_warn = true,
                    _ => {}
                }
                name = None;
                in_path = true;
            }
            SyntaxKind::IDENT if in_path => {
                name = element
                    .into_token()
                    .map(|token| token.text().trim_start_matches("r#").to_string());
            }
            SyntaxKind::EQ | SyntaxKind::TOKEN_TREE => in_path = false,
            _ => {}
        }
    }
}

impl super::SourceInstrumentSyntax {
    #[instrument(level = "debug", skip(attr))]
    pub(crate) fn from_native_attribute(attr: &ast::Attr) -> Option<Self> {
        attr.meta()
            .and_then(|meta| attribute(&meta).instrument_syntax)
    }
}
