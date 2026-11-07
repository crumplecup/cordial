//! Freeze effective Cargo, toolchain and crate-graph evidence in the shared IR.

use super::{compiled, inputs, packages};

use crate::{
    CordialResult, SemanticCargoOptions, SemanticContextGap, SemanticCrateContext,
    SemanticWorkspaceContext,
};
use ra_ap_hir::{Crate, attach_db};
use ra_ap_hir_expand::proc_macro::ProcMacros;
use ra_ap_ide_db::{RootDatabase, base_db::SourceDatabase as RaSourceDatabase};
use ra_ap_proc_macro_api::ProcMacroClient;
use ra_ap_project_model::{ProjectWorkspace, ProjectWorkspaceKind, TargetKind};
use ra_ap_vfs::Vfs;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tracing::instrument;

pub(super) struct WorkspaceInputs {
    root: PathBuf,
    options: SemanticCargoOptions,
    toolchain: Option<String>,
    sysroot: Option<PathBuf>,
    configuration: Vec<crate::SemanticConfigurationInput>,
    build_scripts: Vec<PathBuf>,
    packages: Vec<crate::SemanticPackageContext>,
    environment_digest: [u8; 32],
    gaps: Vec<SemanticContextGap>,
}

impl WorkspaceInputs {
    #[instrument(level = "debug", skip_all, err(level = "warn"))]
    pub(super) fn capture(
        workspace: &ProjectWorkspace,
        options: &SemanticCargoOptions,
        mut gaps: Vec<SemanticContextGap>,
    ) -> CordialResult<Self> {
        if matches!(
            &workspace.kind,
            ProjectWorkspaceKind::Cargo { error: Some(_), .. }
        ) {
            gaps.push(SemanticContextGap::CargoMetadata);
        }
        if workspace.sysroot.is_rust_lib_src_empty()
            || workspace.sysroot.error().is_some()
            || workspace.sysroot.metadata_error().is_some()
        {
            gaps.push(SemanticContextGap::Sysroot);
        }
        Ok(Self {
            root: AsRef::<std::path::Path>::as_ref(workspace.workspace_root()).to_path_buf(),
            options: options.clone(),
            toolchain: workspace.toolchain.as_ref().map(ToString::to_string),
            sysroot: workspace
                .sysroot
                .root()
                .map(|path| AsRef::<std::path::Path>::as_ref(path).to_path_buf()),
            configuration: inputs::configuration(workspace, options)?,
            packages: packages::collect(workspace),
            build_scripts: match &workspace.kind {
                ProjectWorkspaceKind::Cargo { cargo, .. } => cargo
                    .packages()
                    .flat_map(|package| &cargo[package].targets)
                    .filter(|target| cargo[**target].kind == TargetKind::BuildScript)
                    .map(|target| {
                        AsRef::<std::path::Path>::as_ref(&cargo[*target].root).to_path_buf()
                    })
                    .collect(),
                _ => Vec::new(),
            },
            environment_digest: inputs::environment(options),
            gaps,
        })
    }

    #[instrument(level = "trace", skip_all, err(level = "warn"))]
    pub(super) fn finish(
        mut self,
        db: &RootDatabase,
        vfs: &Vfs,
        server: Option<&ProcMacroClient>,
    ) -> CordialResult<SemanticWorkspaceContext> {
        if server.is_none() {
            self.gaps.push(SemanticContextGap::ProcMacroServer);
        }
        let mut crates = Vec::new();
        for krate in Crate::all(db) {
            let Some(root) = vfs.file_path(krate.root_file(db)).as_path() else {
                continue;
            };
            let root = AsRef::<std::path::Path>::as_ref(root).to_path_buf();
            let proc_macro = krate.base().data(db).is_proc_macro;
            if proc_macro
                && ProcMacros::get_for_crate(db, krate.base())
                    .is_none_or(|macros| macros.get_error().is_some())
            {
                self.gaps
                    .push(SemanticContextGap::ProcMacro { root: root.clone() });
            }
            let mut cfg: Vec<_> = krate
                .cfg(db)
                .clone()
                .into_iter()
                .map(|atom| atom.to_string())
                .collect();
            cfg.sort();
            let mut environment: Vec<(String, String)> = krate.base().env(db).clone().into();
            environment.sort();
            let environment_digest = Sha256::digest(serde_json::to_vec(&environment)?).into();
            let mut dependencies: Vec<_> = krate
                .dependencies(db)
                .into_iter()
                .filter_map(|dep| {
                    vfs.file_path(dep.krate.root_file(db))
                        .as_path()
                        .map(|path| AsRef::<std::path::Path>::as_ref(path).to_path_buf())
                })
                .collect();
            dependencies.sort();
            let build_script = self.build_scripts.contains(&root);
            crates.push(SemanticCrateContext::new(
                root,
                krate.edition(db).to_string(),
                cfg,
                environment_digest,
                dependencies,
                proc_macro,
                build_script,
            ));
        }
        crates.sort_by(|a, b| a.root().cmp(b.root()).then(a.cfg().cmp(b.cfg())));
        let mut sources: Vec<_> = vfs
            .iter()
            .map(|(file, path)| {
                (
                    path.to_string(),
                    Sha256::digest(db.file_text(file).text(db).as_bytes()).to_vec(),
                )
            })
            .collect();
        sources.sort();
        let source_digest = Sha256::digest(serde_json::to_vec(&sources)?).into();
        let compiled_inputs = attach_db(db, || compiled::collect(db, vfs, &self.build_scripts));
        let context =
            SemanticWorkspaceContext::builder()
                .identity([0; 32])
                .root(self.root)
                .features(self.options.features().clone())
                .target(self.options.target().clone())
                .all_targets(*self.options.all_targets())
                .test_cfg(*self.options.test_cfg())
                .toolchain(self.toolchain)
                .sysroot(self.sysroot)
                .proc_macro_server(server.map(|server| {
                    AsRef::<std::path::Path>::as_ref(server.server_path()).to_path_buf()
                }))
                .configuration_inputs(self.configuration)
                .compiled_inputs(compiled_inputs)
                .loaded_sources_digest(source_digest)
                .environment_digest(self.environment_digest)
                .packages(self.packages)
                .crates(crates)
                .gaps(self.gaps)
                .build()?;
        let identity = Sha256::digest(serde_json::to_vec(&context)?).into();
        Ok(context.with_identity(identity))
    }
}
