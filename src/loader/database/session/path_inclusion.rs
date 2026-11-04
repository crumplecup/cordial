//! Native workspace splice projection from canonical database inputs.
use super::SourceDatabase;
use crate::ir::{PathInclusionFacts, PathInclusionSource};
use crate::{CordialResult, FileSnapshot};
use ra_ap_hir::{Semantics, attach_db};
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use ra_ap_syntax::ast::{self, HasAttrs, HasModuleItem};
use sha2::{Digest, Sha256};

impl SourceDatabase {
    /// Project module splices for a captured workspace input selection.
    /// Member dependencies and source paths come from workspace IR; module
    /// targets and digests come from this database revision's canonical trees.
    /// Missing inputs fail instead of presenting a partial splice graph.
    #[tracing::instrument(level = "debug", skip(self, context), err(level = "warn"))]
    #[hotpath::measure]
    pub fn project_path_inclusions(
        &self,
        context: &PathInclusionFacts,
    ) -> CordialResult<PathInclusionFacts> {
        attach_db(&self.db, || {
            let sema = Semantics::new(&self.db);
            let mut sources = Vec::new();
            for input in context.sources() {
                let path = input.snapshot().path();
                let id = self.file_id(path)?;
                let edition = sema.attach_first_edition(id);
                if let Some(error) = edition.parse(&self.db).errors().first() {
                    return Err(anyhow::Error::new(error.clone()).into());
                }
                let text = self.db.file_text(id).text(&self.db);
                let tree = sema.parse(edition);
                sources.push(PathInclusionSource::new(
                    input.crate_name().clone(),
                    FileSnapshot::new(path.to_path_buf(), Sha256::digest(text.as_bytes()).into()),
                    targets(tree.items()),
                ));
            }
            Ok(PathInclusionFacts::from_evidence(
                context.members().clone(),
                sources,
            ))
        })
    }
}

#[tracing::instrument(level = "trace", skip(items))]
fn targets(items: impl Iterator<Item = ast::Item>) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        if let ast::Item::Module(module) = item {
            if let Some(target) = module.attrs().find_map(path_value) {
                out.push(target);
            }
            if let Some(list) = module.item_list() {
                out.extend(targets(list.items()));
            }
        }
    }
    out
}

#[tracing::instrument(level = "trace", skip(attr))]
fn path_value(attr: ast::Attr) -> Option<String> {
    if attr.simple_name().as_deref() != Some("path") {
        return None;
    }
    match attr.meta()? {
        ast::Meta::KeyValueMeta(value) => match value.expr()? {
            ast::Expr::Literal(literal) => match literal.kind() {
                ast::LiteralKind::String(value) => value.value().ok().map(|text| text.into_owned()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}
