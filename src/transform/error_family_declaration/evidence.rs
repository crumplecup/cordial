//! Insertion and naming facts come exclusively from shared module evidence.

use std::path::PathBuf;

use crate::error::CordialResult;
use crate::ir::{EdgeKind, IrView, ItemKind, NodeId, NodeKind};
use crate::objects::SourceSpan;
use crate::transform::ErrorDeclarationBlocker as Blocker;

#[derive(derive_getters::Getters)]
pub(super) struct Placement {
    path: PathBuf,
    #[getter(copy)]
    source_digest: [u8; 32],
    #[getter(copy)]
    offset: usize,
    identifiers: Vec<String>,
    stem: String,
}

#[tracing::instrument(level = "trace", skip(ir, functions, root), err(level = "warn"))]
pub(super) fn placement(
    ir: &dyn IrView,
    functions: &[NodeId],
    root: NodeId,
) -> CordialResult<Result<Placement, Blocker>> {
    let Some(node) = ir.node(root) else {
        return Ok(Err(Blocker::InvalidModuleEvidence));
    };
    let (Some(span), Some(facts)) = (node.weight().span(), ir.function_source_syntax(root)?) else {
        return Ok(Err(Blocker::InvalidModuleEvidence));
    };
    let parents = ir.parents(root, EdgeKind::Contains);
    let [module] = parents.as_slice() else {
        return Ok(Err(Blocker::InvalidModuleEvidence));
    };
    let Some(scope) = ir.source_module_scope(*module)? else {
        return Ok(Err(Blocker::InvalidModuleEvidence));
    };
    if scope.opaque()
        || scope.source_digest() != facts.source_digest()
        || !ir.node(*module).is_some_and(|node| {
            node.kind() == &NodeKind::Module
                && node
                    .weight()
                    .span()
                    .as_ref()
                    .is_some_and(|module_span| module_span.file() == span.file())
        })
    {
        return Ok(Err(Blocker::InvalidModuleEvidence));
    }
    for function in functions {
        let Some(node) = ir.node(*function) else {
            return Ok(Err(Blocker::InvalidModuleEvidence));
        };
        let Some(selected) = ir.function_source_syntax(*function)? else {
            return Ok(Err(Blocker::InvalidModuleEvidence));
        };
        if node.kind() != &NodeKind::Item(ItemKind::Fn)
            || ir.parents(*function, EdgeKind::Contains) != parents
            || selected.source_digest() != scope.source_digest()
            || selected.body_range().end() > scope.insertion_offset()
            || !node
                .weight()
                .span()
                .as_ref()
                .is_some_and(|selected_span| selected_span.file() == span.file())
        {
            return Ok(Err(Blocker::InvalidModuleEvidence));
        }
    }
    let Some(name) = node.weight().name().as_deref() else {
        return Ok(Err(Blocker::UnsupportedName));
    };
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Ok(Err(Blocker::UnsupportedName));
    }
    let stem = name
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (first, rest) = part.split_at(1);
            format!("{}{rest}", first.to_ascii_uppercase())
        })
        .collect();
    Ok(Ok(Placement {
        path: span.file().to_path_buf(),
        source_digest: scope.source_digest(),
        offset: scope.insertion_offset(),
        identifiers: scope.identifiers().clone(),
        stem,
    }))
}
