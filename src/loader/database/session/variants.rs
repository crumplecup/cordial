//! Load the base Cargo configuration and its proof-cfg variants into one database.
//!
//! rust-analyzer resolves only code active under a crate's cfg set. Proof
//! harnesses and verifier-gated helpers are inactive in the ordinary build, so
//! a second crate graph enables the verifier cfgs for the workspace's own
//! packages. `CrateGraphBuilder::extend` merges it into the first and
//! deduplicates identical crates: the sysroot, dependencies and every file are
//! shared, files parse once, and only the workspace crates gain variants.
//! Upstream's `load_workspace_into_db` loads exactly one graph, so this module
//! repeats its steps with the merge added.

use crate::CordialResult;
use crossbeam_channel::{Receiver, unbounded};
use ra_ap_cfg::{CfgAtom, CfgDiff};
use ra_ap_hir_expand::{intern::Symbol, proc_macro::ProcMacrosBuilder};
use ra_ap_ide_db::{
    ChangeWithProcMacros, FxHashMap, RootDatabase,
    base_db::{ProcMacroLoadingError, ProcMacroPaths},
};
use ra_ap_load_cargo::{ProjectFolders, load_proc_macro};
use ra_ap_proc_macro_api::ProcMacroClient;
use ra_ap_project_model::{ProjectWorkspace, ProjectWorkspaceKind};
use ra_ap_vfs::{
    AbsPath, Change, FileExcluded, Vfs, VfsPath,
    loader::{Handle, LoadingProgress, Message},
};
use tracing::instrument;

/// Load `workspace` and, when `proof_cfgs` is non-empty, a variant of its own
/// packages with those cfg flags enabled, into one database.
#[instrument(level = "debug", skip(workspace, extra_env, db), err(level = "warn"))]
#[hotpath::measure]
pub(super) fn load_into_db(
    workspace: ProjectWorkspace,
    proof_cfgs: &[String],
    extra_env: &FxHashMap<String, Option<String>>,
    db: &mut RootDatabase,
) -> CordialResult<(Vfs, Option<ProcMacroClient>)> {
    let (sender, receiver) = unbounded();
    let mut vfs = Vfs::default();
    let mut loader = Box::new(ra_ap_vfs_notify::NotifyHandle::spawn(sender));
    let server = proc_macro_server(&workspace, extra_env);
    let mut load = |path: &AbsPath| {
        let contents = loader.load_sync(path);
        let path = VfsPath::from(path.to_path_buf());
        vfs.set_file_contents(path.clone(), contents);
        vfs.file_id(&path)
            .and_then(|(file, excluded)| (excluded == FileExcluded::No).then_some(file))
    };
    let (mut graph, mut paths) = workspace.to_crate_graph(&mut load, extra_env);
    if !proof_cfgs.is_empty() {
        let (other, mut other_paths) =
            proof_variant(&workspace, proof_cfgs).to_crate_graph(&mut load, extra_env);
        graph.extend(other, &mut other_paths);
        paths.extend(other_paths);
    }
    let proc_macros = proc_macros(paths, &server);
    let folders = ProjectFolders::new(std::slice::from_ref(&workspace), &[], None);
    loader.set_config(ra_ap_vfs::loader::Config {
        load: folders.load,
        watch: vec![],
        version: 0,
    });
    let mut change = ChangeWithProcMacros::default();
    db.enable_proc_attr_macros();
    wait_for_roots(&receiver, &mut vfs);
    for (_, file) in vfs.take_changes() {
        if let Change::Create(bytes, _) | Change::Modify(bytes, _) = file.change
            && let Ok(text) = String::from_utf8(bytes)
        {
            change.change_file(file.file_id, Some(text));
        }
    }
    change.set_roots(folders.source_root_config.partition(&vfs));
    change.set_crate_graph(graph);
    change.set_proc_macros(proc_macros);
    db.apply_change(change);
    Ok((vfs, server.and_then(Result::ok)))
}

/// The workspace with `cfgs` enabled for each of its own (local) packages.
/// Dependencies and the sysroot keep their cfg set and so deduplicate away.
#[instrument(level = "debug", skip(workspace))]
fn proof_variant(workspace: &ProjectWorkspace, cfgs: &[String]) -> ProjectWorkspace {
    let enable: Vec<CfgAtom> = cfgs
        .iter()
        .map(|name| CfgAtom::Flag(Symbol::intern(name)))
        .collect();
    let mut variant = workspace.clone();
    if let ProjectWorkspaceKind::Cargo { cargo, .. } = &workspace.kind {
        variant.cfg_overrides.selective = cargo
            .packages()
            .filter(|package| cargo[*package].is_local)
            .map(|package| {
                (
                    cargo[package].name.clone(),
                    CfgDiff::new(enable.clone(), Vec::new()),
                )
            })
            .collect();
    }
    variant
}

/// Start the sysroot's proc-macro server. The error keeps its source chain; the
/// context records the gap and the log carries the cause.
#[instrument(level = "debug", skip(workspace, extra_env))]
fn proc_macro_server(
    workspace: &ProjectWorkspace,
    extra_env: &FxHashMap<String, Option<String>>,
) -> Option<anyhow::Result<ProcMacroClient>> {
    let server = workspace.find_sysroot_proc_macro_srv()?.and_then(|path| {
        Ok(ProcMacroClient::spawn(
            &path,
            extra_env,
            workspace.toolchain.as_ref(),
            1,
        )?)
    });
    if let Err(error) = &server {
        tracing::warn!(error = ?error, "proc-macro server unavailable");
    }
    Some(server)
}

#[instrument(level = "debug", skip_all)]
fn proc_macros(
    paths: ProcMacroPaths,
    server: &Option<anyhow::Result<ProcMacroClient>>,
) -> ProcMacrosBuilder {
    let server = match server {
        Some(Ok(server)) => Ok(server),
        // Upstream's error type carries text only; the cause was logged above.
        Some(Err(error)) => Err(ProcMacroLoadingError::ProcMacroSrvError(
            format!("{error:#}").into_boxed_str(),
        )),
        None => Err(ProcMacroLoadingError::ProcMacroSrvError(
            "proc-macro-srv is not running, workspace is missing a sysroot".into(),
        )),
    };
    paths
        .into_iter()
        .map(|(krate, path)| {
            (
                krate,
                path.and_then(|(_, path)| {
                    server
                        .as_ref()
                        .map_err(Clone::clone)
                        .and_then(|server| load_proc_macro(server, &path, &[]))
                }),
            )
        })
        .collect()
}

/// Block until the file loader has delivered every source root.
#[instrument(level = "debug", skip_all)]
fn wait_for_roots(receiver: &Receiver<Message>, vfs: &mut Vfs) {
    for message in receiver {
        match message {
            Message::Progress { n_done, .. } => {
                if n_done == LoadingProgress::Finished {
                    break;
                }
            }
            Message::Loaded { files } | Message::Changed { files } => {
                for (path, contents) in files {
                    vfs.set_file_contents(path.into(), contents);
                }
            }
        }
    }
}
