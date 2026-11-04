//! Loader-only assembly of a crate module tree from captured file bodies.
use super::{
    VisibilityBody, VisibilityChildBody, VisibilityFacts, VisibilityKind, VisibilityModule,
};
use crate::{CordialError, CordialResult, FileSnapshot};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[tracing::instrument(level = "debug", skip(read), err(level = "warn"))]
pub(crate) fn collect(
    root: &Path,
    mut read: impl FnMut(&Path) -> CordialResult<(VisibilityBody, FileSnapshot)>,
) -> CordialResult<VisibilityFacts> {
    let mut facts = VisibilityFacts::default();
    let Some(file) = [
        root.join("src/lib.rs"),
        root.join("src/main.rs"),
        root.join("lib.rs"),
    ]
    .into_iter()
    .find(|path| path.is_file()) else {
        return Ok(facts);
    };
    let header = Header {
        path: "crate".into(),
        file,
        line: 1,
        declared_vis: VisibilityKind::Pub,
        parent_declared_pub: true,
        ancestors_all_pub: true,
        is_crate_root: true,
    };
    facts.root = Some(file_tree(
        header,
        &mut read,
        &mut facts.snapshots,
        &mut BTreeSet::new(),
    )?);
    facts.snapshots.sort_by(|a, b| a.path().cmp(b.path()));
    facts.snapshots.dedup();
    Ok(facts)
}

struct Header {
    path: String,
    file: PathBuf,
    line: u32,
    declared_vis: VisibilityKind,
    parent_declared_pub: bool,
    ancestors_all_pub: bool,
    is_crate_root: bool,
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn file_tree(
    mut header: Header,
    read: &mut impl FnMut(&Path) -> CordialResult<(VisibilityBody, FileSnapshot)>,
    snapshots: &mut Vec<FileSnapshot>,
    active: &mut BTreeSet<PathBuf>,
) -> CordialResult<VisibilityModule> {
    let identity = header.file.canonicalize()?;
    if !active.insert(identity.clone()) {
        return Err(CordialError::not_found(
            format!("acyclic visibility module tree: {}", header.file.display()).into(),
        ));
    }
    let (body, snapshot) = read(&header.file)?;
    snapshots.push(snapshot);
    header.line = 1;
    let node = body_tree(body, header, read, snapshots, active)?;
    active.remove(&identity);
    Ok(node)
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn body_tree(
    body: VisibilityBody,
    header: Header,
    read: &mut impl FnMut(&Path) -> CordialResult<(VisibilityBody, FileSnapshot)>,
    snapshots: &mut Vec<FileSnapshot>,
    active: &mut BTreeSet<PathBuf>,
) -> CordialResult<VisibilityModule> {
    let mut children = Vec::new();
    for child in body.children {
        let child_header = Header {
            path: format!("{}::{}", header.path, child.name),
            file: header.file.clone(),
            line: child.line,
            declared_vis: child.visibility,
            parent_declared_pub: header.declared_vis.is_unrestricted_pub(),
            ancestors_all_pub: header.ancestors_all_pub && child.visibility.is_unrestricted_pub(),
            is_crate_root: false,
        };
        let node = match child.body {
            VisibilityChildBody::Inline(body) => {
                body_tree(body, child_header, read, snapshots, active)?
            }
            VisibilityChildBody::External(path) => {
                if let Some(file) = resolve(&header.file, &child.name, path.as_deref()) {
                    file_tree(
                        Header {
                            file,
                            ..child_header
                        },
                        read,
                        snapshots,
                        active,
                    )?
                } else {
                    // An unresolved declaration is explicit empty evidence, as in the existing scanner.
                    body_tree(
                        VisibilityBody::default(),
                        child_header,
                        read,
                        snapshots,
                        active,
                    )?
                }
            }
        };
        children.push(node);
    }
    Ok(VisibilityModule {
        path: header.path,
        file: header.file,
        line: header.line,
        declared_vis: header.declared_vis,
        parent_declared_pub: header.parent_declared_pub,
        ancestors_all_pub: header.ancestors_all_pub,
        is_crate_root: header.is_crate_root,
        leaf_pub: body.leaf_pub,
        leaf_crate: body.leaf_crate,
        children,
    })
}

#[tracing::instrument(level = "trace")]
fn resolve(parent: &Path, name: &str, explicit: Option<&str>) -> Option<PathBuf> {
    let dir = parent.parent()?;
    if let Some(explicit) = explicit {
        let file = dir.join(explicit);
        return file.is_file().then_some(file);
    }
    let stem = parent.file_stem()?.to_str()?;
    let dir = if matches!(stem, "lib" | "main" | "mod") {
        dir.to_path_buf()
    } else {
        dir.join(stem)
    };
    [
        dir.join(format!("{name}.rs")),
        dir.join(name).join("mod.rs"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}
