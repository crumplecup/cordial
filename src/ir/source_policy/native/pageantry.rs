//! Item ordering and barrel declarations from canonical syntax.
use std::path::Path;

use crate::etiquettes::pageantry::{
    PageantryRuleId, PageantrySiteRecord, PageantrySourceSite, ProcMacroShim,
};
use crate::{CordialResult, loader::DatabaseSource};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasModuleItem, HasName},
};

#[tracing::instrument(level = "debug", skip(file, prefix), err(level = "warn"))]
pub(super) fn collect(
    file: &DatabaseSource<'_>,
    root: &Path,
    prefix: &[String],
) -> CordialResult<Vec<PageantrySourceSite>> {
    let mut out = Vec::new();
    items(file.tree().items(), file, root, prefix, &mut out)?;
    Ok(out)
}

#[tracing::instrument(level = "debug", skip(iter, file, prefix, out), err(level = "warn"))]
fn items(
    iter: impl Iterator<Item = ast::Item>,
    file: &DatabaseSource<'_>,
    root: &Path,
    prefix: &[String],
    out: &mut Vec<PageantrySourceSite>,
) -> CordialResult<()> {
    let barrel = matches!(
        file.path().file_name().and_then(|name| name.to_str()),
        Some("lib.rs" | "mod.rs")
    );
    let mut body_started = false;
    for item in iter {
        if super::attributes(item.syntax()).any(|attr| super::test_gate(&attr)) {
            continue;
        }
        if barrel && let Some((rule, line, snippet, shim)) = declaration(&item, file)? {
            out.push(site(file, root, prefix, rule, line, snippet, shim)?);
        }
        match item {
            ast::Item::Module(module) => {
                if let Some(list) = module.item_list() {
                    let mut nested = prefix.to_vec();
                    nested.push(name(&module)?.0);
                    items(list.items(), file, root, &nested, out)?;
                }
            }
            ast::Item::Use(_) | ast::Item::ExternCrate(_) => {}
            ast::Item::Trait(item) => {
                if body_started {
                    let (name, offset) = name(&item)?;
                    out.push(site(
                        file,
                        root,
                        prefix,
                        PageantryRuleId::Trait001,
                        file.line(offset),
                        format!("trait {name}"),
                        None,
                    )?);
                }
            }
            _ => body_started = true,
        }
    }
    Ok(())
}

type Declaration = (PageantryRuleId, u32, String, Option<ProcMacroShim>);

#[tracing::instrument(level = "trace", skip(item, file), err(level = "warn"))]
fn declaration(item: &ast::Item, file: &DatabaseSource<'_>) -> CordialResult<Option<Declaration>> {
    if let ast::Item::Fn(function) = item
        && let Some(attr) = super::attributes(item.syntax()).find_map(|attr| {
            let path = attr.path()?;
            if path.qualifier().is_some()
                || path
                    .segment()
                    .is_some_and(|part| part.coloncolon_token().is_some())
            {
                return None;
            }
            let text = path.segment()?.name_ref()?.text().to_string();
            matches!(
                text.as_str(),
                "proc_macro" | "proc_macro_derive" | "proc_macro_attribute"
            )
            .then_some(text)
        })
    {
        let (name, offset) = name(function)?;
        let body = function
            .body()
            .and_then(|body| body.stmt_list())
            .ok_or_else(|| crate::CordialError::not_found("proc-macro body".into()))?;
        let open = file.line(body.syntax().text_range().start());
        let close = file.line(body.syntax().text_range().end() - ra_ap_syntax::TextSize::from(1));
        return Ok(Some((
            PageantryRuleId::BarrelShim001,
            file.line(offset),
            name,
            Some(ProcMacroShim::new(
                attr,
                close.saturating_sub(open).saturating_sub(1) as usize,
            )),
        )));
    }
    let (offset, snippet) = match item {
        ast::Item::Use(_) | ast::Item::ExternCrate(_) | ast::Item::Module(_) => return Ok(None),
        ast::Item::Fn(item) => named(item, "fn")?,
        ast::Item::Struct(item) => named(item, "struct")?,
        ast::Item::Enum(item) => named(item, "enum")?,
        ast::Item::Union(item) => named(item, "union")?,
        ast::Item::Trait(item) => named(item, "trait")?,
        ast::Item::TypeAlias(item) => named(item, "type")?,
        ast::Item::Const(item) => match item.underscore_token() {
            Some(token) => (token.text_range().start(), "const _".into()),
            None => named(item, "const")?,
        },
        ast::Item::Static(item) => named(item, "static")?,
        ast::Item::Impl(item) => {
            let label = match item.self_ty() {
                Some(ast::Type::PathType(ty)) => ty
                    .path()
                    .and_then(|path| path.segment())
                    .and_then(|part| part.name_ref())
                    .map(|name| format!("impl {}", name.text()))
                    .unwrap_or_else(|| "impl".into()),
                _ => "impl".into(),
            };
            (
                item.impl_token()
                    .ok_or_else(|| crate::CordialError::not_found("impl token".into()))?
                    .text_range()
                    .start(),
                label,
            )
        }
        ast::Item::MacroRules(item) => named(item, "macro")?,
        ast::Item::MacroDef(item) => named(item, "macro")?,
        ast::Item::MacroCall(item) => {
            let path = item
                .path()
                .ok_or_else(|| crate::CordialError::not_found("macro path".into()))?;
            let name = path
                .segment()
                .and_then(|part| part.name_ref())
                .ok_or_else(|| crate::CordialError::not_found("macro name".into()))?;
            (
                path.syntax().text_range().start(),
                format!("macro {}", name.text()),
            )
        }
        ast::Item::ExternBlock(item) => (
            item.abi()
                .and_then(|abi| abi.extern_token())
                .ok_or_else(|| crate::CordialError::not_found("extern token".into()))?
                .text_range()
                .start(),
            "extern block".into(),
        ),
        _ => return Ok(Some((PageantryRuleId::Barrel001, 1, "item".into(), None))),
    };
    Ok(Some((
        PageantryRuleId::Barrel001,
        file.line(offset),
        snippet,
        None,
    )))
}

#[tracing::instrument(level = "trace", skip(owner), err(level = "warn"))]
fn name(owner: &impl HasName) -> CordialResult<(String, ra_ap_syntax::TextSize)> {
    owner
        .name()
        .map(|name| (name.text().to_string(), name.syntax().text_range().start()))
        .ok_or_else(|| crate::CordialError::not_found("pageantry declaration name".into()))
}

#[tracing::instrument(level = "trace", skip(owner), err(level = "warn"))]
fn named(owner: &impl HasName, prefix: &str) -> CordialResult<(ra_ap_syntax::TextSize, String)> {
    let (name, offset) = name(owner)?;
    Ok((offset, format!("{prefix} {name}")))
}

#[tracing::instrument(level = "trace", skip(file, prefix, rule, shim), err(level = "warn"))]
fn site(
    file: &DatabaseSource<'_>,
    root: &Path,
    prefix: &[String],
    rule: PageantryRuleId,
    line: u32,
    snippet: String,
    shim: Option<ProcMacroShim>,
) -> CordialResult<PageantrySourceSite> {
    Ok(PageantrySourceSite::new(
        PageantrySiteRecord::builder()
            .rule_id(rule)
            .context(if prefix.is_empty() {
                "<crate>".into()
            } else {
                prefix.join("::")
            })
            .file(file.path().strip_prefix(root)?.to_path_buf())
            .line(line)
            .snippet(snippet)
            .build()?,
        shim,
    ))
}
