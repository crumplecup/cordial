//! Size facts from the database tree, before thresholds or generated-file policy.
use std::path::Path;

use crate::etiquettes::modularity::{ModularityKind, ModularitySiteRecord};
use crate::{CordialResult, loader::DatabaseSource};
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasModuleItem, HasName},
};

#[tracing::instrument(level = "debug", skip(file, prefix), err(level = "warn"))]
pub(super) fn collect(
    file: &DatabaseSource<'_>,
    root: &Path,
    prefix: &[String],
) -> CordialResult<Vec<ModularitySiteRecord>> {
    let lines = u32::try_from(file.source().lines().count()).unwrap_or(u32::MAX);
    let mut out = vec![
        record(
            file,
            root,
            ModularityKind::File,
            String::new(),
            1,
            lines,
            false,
        )?,
        record(
            file,
            root,
            ModularityKind::ModuleSize,
            super::context::joined(prefix.to_vec()),
            1,
            lines,
            false,
        )?,
    ];
    let mut names = Vec::new();
    items(
        file.tree().items(),
        file,
        root,
        prefix,
        &mut names,
        &mut out,
    )?;
    out.push(record(
        file,
        root,
        ModularityKind::TypesPerFile,
        names.join(", "),
        1,
        u32::try_from(names.len()).unwrap_or(u32::MAX),
        false,
    )?);
    for function in file.tree().syntax().descendants().filter_map(ast::Fn::cast) {
        let node = function.syntax();
        if !super::outside_macro(node)
            || node.ancestors().any(|owner| {
                ast::ClosureExpr::can_cast(owner.kind())
                    || ((ast::Module::can_cast(owner.kind())
                        || ast::Fn::can_cast(owner.kind())
                        || ast::Impl::can_cast(owner.kind())
                        || ast::Trait::can_cast(owner.kind()))
                        && test_only(&owner))
            })
        {
            continue;
        }
        if let Some(body) = function.body().and_then(|body| body.stmt_list()) {
            let start = file.line(body.syntax().text_range().start());
            let end = file.line(body.syntax().text_range().end() - ra_ap_syntax::TextSize::from(1));
            out.push(record(
                file,
                root,
                ModularityKind::Function,
                super::context::function_context(node, prefix),
                start,
                end.saturating_sub(start).saturating_add(1),
                false,
            )?);
        }
    }
    Ok(out)
}

#[tracing::instrument(
    level = "debug",
    skip(iter, file, prefix, names, out),
    err(level = "warn")
)]
fn items(
    iter: impl Iterator<Item = ast::Item>,
    file: &DatabaseSource<'_>,
    root: &Path,
    prefix: &[String],
    names: &mut Vec<String>,
    out: &mut Vec<ModularitySiteRecord>,
) -> CordialResult<()> {
    for item in iter {
        if test_only(item.syntax()) {
            continue;
        }
        match item {
            ast::Item::Module(module) => {
                if let Some(list) = module.item_list() {
                    let mut nested = prefix.to_vec();
                    nested.push(name(&module)?);
                    let start = file.line(module.syntax().text_range().start());
                    let end = file
                        .line(module.syntax().text_range().end() - ra_ap_syntax::TextSize::from(1));
                    out.push(record(
                        file,
                        root,
                        ModularityKind::ModuleSize,
                        super::context::joined(nested.clone()),
                        start,
                        end.saturating_sub(start).saturating_add(1),
                        true,
                    )?);
                    items(list.items(), file, root, &nested, names, out)?;
                }
            }
            ast::Item::Struct(item) => names.push(name(&item)?),
            ast::Item::Enum(item) => names.push(name(&item)?),
            ast::Item::Union(item) => names.push(name(&item)?),
            ast::Item::Trait(item) if item.eq_token().is_none() => names.push(name(&item)?),
            _ => {}
        }
    }
    Ok(())
}

#[tracing::instrument(level = "trace", skip(owner), err(level = "warn"))]
fn name(owner: &impl HasName) -> CordialResult<String> {
    owner
        .name()
        .map(|name| name.text().to_string())
        .ok_or_else(|| crate::CordialError::not_found("modularity declaration name".into()))
}

#[tracing::instrument(level = "trace", skip(node))]
fn test_only(node: &SyntaxNode) -> bool {
    super::attributes(node).any(|attr| super::test_gate(&attr))
}

#[tracing::instrument(level = "trace", skip(file, kind), err(level = "warn"))]
fn record(
    file: &DatabaseSource<'_>,
    root: &Path,
    kind: ModularityKind,
    context: String,
    line: u32,
    lines: u32,
    inline: bool,
) -> CordialResult<ModularitySiteRecord> {
    ModularitySiteRecord::builder()
        .kind(kind)
        .context(context)
        .file(file.path().strip_prefix(root)?.to_path_buf())
        .line(line)
        .lines(lines)
        .inline(inline)
        .build()
}
