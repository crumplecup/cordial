//! Effectful workspace loading stays in the driver, outside tracked queries.

use super::{context, inputs, packages};

use super::SourceDatabase;
use crate::{CordialResult, SemanticCargoOptions, SemanticContextGap};
use ra_ap_ide_db::RootDatabase;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, load_workspace_into_db};
use ra_ap_project_model::{ProjectManifest, ProjectWorkspace};
use ra_ap_vfs::AbsPathBuf;
use std::path::Path;
use tracing::instrument;

impl SourceDatabase {
    /// Load the default Cargo configuration, build outputs and proc macros.
    #[instrument(level = "debug", skip(root), err(level = "warn"))]
    #[hotpath::measure]
    pub fn load(root: &Path) -> CordialResult<Self> {
        Self::load_with(root, &SemanticCargoOptions::default())
    }

    /// Load one explicit Cargo configuration into the canonical database.
    /// Cargo diagnostics become context gaps; a failed workspace load is an
    /// error. Incomplete loading never proves a missing semantic capability.
    #[instrument(level = "debug", skip(root, options), err(level = "warn"))]
    #[hotpath::measure]
    pub fn load_with(root: &Path, options: &SemanticCargoOptions) -> CordialResult<Self> {
        let root = root.canonicalize()?;
        // canonicalize established absoluteness; check UTF-8 before the
        // upstream path constructor's assertion boundary.
        root.to_str()
            .ok_or_else(|| crate::CordialError::not_found(root.clone()))?;
        let path = AbsPathBuf::assert_utf8(root.clone());
        let manifest = ProjectManifest::discover_single(&path)?;
        let config = options.cargo_config();
        let progress = |message: String| tracing::debug!(%message, "semantic workspace load");
        let mut workspace = ProjectWorkspace::load(manifest, &config, &progress)?;
        let mut gaps = Vec::new();
        let scripts = workspace.run_build_scripts(&config, &progress)?;
        if scripts.error().is_some() {
            gaps.push(SemanticContextGap::BuildScripts);
        }
        workspace.set_build_scripts(scripts);
        let context = context::WorkspaceInputs::capture(&workspace, options, gaps)?;
        let external_packages = packages::collect_external(&workspace);
        let load_config = LoadCargoConfig {
            load_out_dirs_from_check: true,
            with_proc_macro_server: ProcMacroServerChoice::Sysroot,
            prefill_caches: false,
            num_worker_threads: 1,
            proc_macro_processes: 1,
        };
        let mut db = RootDatabase::new(None);
        let (vfs, server) =
            load_workspace_into_db(workspace, &config.extra_env, &load_config, &mut db)?;
        let context = context.finish(&db, &vfs, server.as_ref())?;
        Ok(Self {
            db,
            vfs,
            revision: 1,
            workspace: Some(context),
            external_packages,
            cargo_options: Some(options.clone()),
            proc_macro_server: server,
            #[cfg(feature = "cfg_hygiene")]
            workspace_root: Some(root),
        })
    }

    /// Rebuild Cargo context after manifest, configuration, environment,
    /// build-script or proc-macro changes. Replacement is atomic: a failed
    /// load leaves this session and its virtual source revisions untouched.
    #[instrument(level = "debug", skip(self, root, options), err(level = "warn"))]
    pub fn reload(&mut self, root: &Path, options: &SemanticCargoOptions) -> CordialResult<()> {
        let mut replacement = Self::load_with(root, options)?;
        replacement.revision = self.revision + 1;
        *self = replacement;
        Ok(())
    }

    /// Virtual edits cannot refresh a Cargo manifest or a compiled macro.
    #[instrument(level = "debug", skip(self, path), err(level = "warn"))]
    pub(super) fn check_virtual_input(&self, path: &Path) -> CordialResult<()> {
        let path = path.canonicalize()?;
        if let Some(context) = &self.workspace {
            let cargo_input = context.configuration_inputs().iter().any(|input| {
                input.path() == &path
                    || input
                        .path()
                        .canonicalize()
                        .is_ok_and(|resolved| resolved == path)
            });
            let compiled_input = context.compiled_inputs().iter().any(|input| {
                input.path() == &path
                    || input
                        .path()
                        .canonicalize()
                        .is_ok_and(|resolved| resolved == path)
            });
            if cargo_input || compiled_input {
                return Err(crate::CordialError::semantic_context_input(path));
            }
        }
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    pub(in crate::loader::database) fn projection_context(
        &self,
    ) -> CordialResult<crate::SemanticWorkspaceContext> {
        let context = self.workspace.as_ref().ok_or_else(|| {
            crate::CordialError::workspace_source_facts_missing("semantic_workspace_context")
        })?;
        inputs::validate(context)?;
        if self
            .cargo_options
            .as_ref()
            .is_some_and(|options| &inputs::environment(options) != context.environment_digest())
        {
            return Err(crate::CordialError::semantic_context_input(
                context.root().clone(),
            ));
        }
        if let Some(server) = &self.proc_macro_server
            && server.exited().is_some()
        {
            return Err(crate::CordialError::semantic_context_input(
                AsRef::<std::path::Path>::as_ref(server.server_path()).to_path_buf(),
            ));
        }
        Ok(context.clone())
    }
}
