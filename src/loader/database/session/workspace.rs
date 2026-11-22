//! Effectful workspace loading stays in the driver, outside tracked queries.

use super::{context, inputs, packages};

use super::SourceDatabase;
use crate::loader::database::{Reporter, SavedState};
use crate::{CordialResult, SemanticCargoOptions, SemanticContextGap};
use ra_ap_ide_db::RootDatabase;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, load_workspace_into_db_with_vfs};
use ra_ap_project_model::CargoConfig;
use ra_ap_project_model::{ProjectManifest, ProjectWorkspace};
use ra_ap_vfs::AbsPathBuf;
use std::path::Path;
use tracing::instrument;

impl SourceDatabase {
    /// Load the default Cargo configuration, build outputs and proc macros.
    #[instrument(level = "debug", skip(root), err(level = "warn"))]
    #[hotpath::measure]
    pub fn load(root: &Path) -> CordialResult<Self> {
        Self::load_reporting(root, &SemanticCargoOptions::default(), &Reporter::default())
    }

    /// Load one explicit Cargo configuration into the canonical database.
    /// Cargo diagnostics become context gaps; a failed workspace load is an
    /// error. Incomplete loading never proves a missing semantic capability.
    #[instrument(level = "debug", skip(root, options), err(level = "warn"))]
    #[hotpath::measure]
    pub fn load_with(root: &Path, options: &SemanticCargoOptions) -> CordialResult<Self> {
        Self::load_reporting(root, options, &Reporter::default())
    }

    /// [`Self::load_with`], saying each phase through `reporter` as it starts.
    #[instrument(level = "debug", skip(root, options, reporter), err(level = "warn"))]
    #[hotpath::measure]
    pub fn load_reporting(
        root: &Path,
        options: &SemanticCargoOptions,
        reporter: &Reporter,
    ) -> CordialResult<Self> {
        Self::load_saved_reporting(root, options, reporter, None)
    }

    /// [`Self::load_with`], starting from the database a previous run saved in `saved`
    /// when there is one this build can use, and saving this session's database there once
    /// its ordinary projection is done. A saved state that cannot be used is said so and
    /// the load is cold.
    #[instrument(level = "debug", skip(root, options, saved), err(level = "warn"))]
    #[hotpath::measure]
    pub fn load_saved(
        root: &Path,
        options: &SemanticCargoOptions,
        saved: Option<SavedState>,
    ) -> CordialResult<Self> {
        Self::load_saved_reporting(root, options, &Reporter::default(), saved)
    }

    /// [`Self::load_saved`], saying each phase through `reporter` as it starts.
    #[instrument(
        level = "debug",
        skip(root, options, reporter, saved),
        err(level = "warn")
    )]
    #[hotpath::measure]
    pub(crate) fn load_saved_reporting(
        root: &Path,
        options: &SemanticCargoOptions,
        reporter: &Reporter,
        saved: Option<SavedState>,
    ) -> CordialResult<Self> {
        super::super::eviction::cap_from_environment();
        let root = root.canonicalize()?;
        // canonicalize established absoluteness; check UTF-8 before the
        // upstream path constructor's assertion boundary.
        root.to_str()
            .ok_or_else(|| crate::CordialError::not_found(root.clone()))?;
        let config = options.cargo_config();
        reporter.say("Reading Cargo metadata");
        let mut workspace = discover(&root, &config)?;
        crate::process_memory::log("cargo metadata read");
        reporter.say("Running build scripts");
        let gaps = run_build_scripts(&mut workspace, &config)?;
        crate::process_memory::log("build scripts run");
        let context = context::WorkspaceInputs::capture(&workspace, options, gaps)?;
        let external_packages = packages::collect_external(&workspace);
        reporter.say("Loading the workspace into rust-analyzer");
        let Loaded {
            db,
            vfs,
            server,
            kept,
        } = load_database(workspace, &config, saved.as_ref())?;
        crate::process_memory::log("rust-analyzer database loaded");
        let context = context.finish(&db, &vfs, server.as_ref())?;
        crate::process_memory::log("workspace loaded");
        Ok(Self {
            db,
            vfs,
            revision: 1,
            workspace: Some(context),
            external_packages,
            cargo_options: Some(options.clone()),
            proc_macro_server: server,
            restored: kept.is_some(),
            resume_contexts: None,
            context_crates: kept.unwrap_or_default(),
            reporter: reporter.clone(),
            saved,
            saved_once: false,
            persisted_at_load: salsa::persisted_executions(),
            spill: None,
            #[cfg(feature = "cfg_hygiene")]
            workspace_root: Some(root),
        })
    }

    /// Rebuild Cargo context after manifest, configuration, environment,
    /// build-script or proc-macro changes. Replacement is atomic: a failed
    /// load leaves this session and its virtual source revisions untouched.
    #[instrument(level = "debug", skip(self, root, options), err(level = "warn"))]
    #[hotpath::measure]
    pub fn reload(&mut self, root: &Path, options: &SemanticCargoOptions) -> CordialResult<()> {
        let mut replacement = Self::load_with(root, options)?;
        replacement.revision = self.revision + 1;
        *self = replacement;
        Ok(())
    }

    /// Virtual edits cannot refresh a Cargo manifest or a compiled macro.
    #[instrument(level = "debug", skip(self, path), err(level = "warn"))]
    #[hotpath::measure]
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
    #[hotpath::measure(impl_type = "SourceDatabase")]
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

/// Discover the manifest and read `cargo metadata` and the sysroot.
#[instrument(level = "debug", skip(root, config), err(level = "warn"))]
#[hotpath::measure]
fn discover(root: &Path, config: &CargoConfig) -> CordialResult<ProjectWorkspace> {
    let path = AbsPathBuf::assert_utf8(root.to_path_buf());
    let manifest = ProjectManifest::discover_single(&path)?;
    let progress = |message: String| tracing::debug!(%message, "semantic workspace load");
    Ok(ProjectWorkspace::load(manifest, config, &progress)?)
}

/// Run Cargo's build scripts for their outputs; failure becomes a context gap.
#[instrument(level = "debug", skip(workspace, config), err(level = "warn"))]
#[hotpath::measure]
fn run_build_scripts(
    workspace: &mut ProjectWorkspace,
    config: &CargoConfig,
) -> CordialResult<Vec<SemanticContextGap>> {
    let progress = |message: String| tracing::debug!(%message, "semantic workspace load");
    let mut gaps = Vec::new();
    let scripts = workspace.run_build_scripts(config, &progress)?;
    if scripts.error().is_some() {
        gaps.push(SemanticContextGap::BuildScripts);
    }
    workspace.set_build_scripts(scripts);
    Ok(gaps)
}

/// Load the workspace into a fresh database with the proc-macro server.
#[instrument(level = "debug", skip(workspace, config, saved), err(level = "warn"))]
#[hotpath::measure]
fn load_database(
    workspace: ProjectWorkspace,
    config: &CargoConfig,
    saved: Option<&SavedState>,
) -> CordialResult<Loaded> {
    let load_config = LoadCargoConfig {
        load_out_dirs_from_check: true,
        with_proc_macro_server: ProcMacroServerChoice::Sysroot,
        prefill_caches: false,
        num_worker_threads: 1,
        proc_macro_processes: 1,
    };
    let (mut db, mut vfs, restored) = match saved.map(SavedState::restore) {
        Some(Ok(restored)) => {
            tracing::info!(
                target: "cordial::cache",
                paths = restored.paths.len(),
                contexts = restored.contexts.len(),
                "the saved analysis state was restored"
            );
            crate::host::event(
                "analysis state: restored from the last run; the workspace is loaded onto it"
                    .to_owned(),
            );
            // Hand out the ids the saved database was built with: the same path, the same
            // id, in whatever order the loader's threads deliver the files.
            let mut vfs = ra_ap_vfs::Vfs::default();
            for path in restored.paths {
                vfs.set_file_contents(path, None);
            }
            // A restored memo can depend on a persisted query that nothing has called yet in this
            // process (a definition map's fingerprint), and verifying it needs the query set up.
            let database = restored.database;
            let semantic: &dyn super::super::queries::SemanticDatabase = &database;
            let _ = super::super::queries::SemanticDatabase::zalsa_register_downcaster(semantic);
            // The same for the queries rust-analyzer saves (what is in a crate's lang items and its impl indexes).
            let source: &dyn ra_ap_ide_db::base_db::SourceDatabase = &database;
            let _ = ra_ap_ide_db::base_db::SourceDatabase::zalsa_register_downcaster(source);
            let types: &dyn ra_ap_hir_ty::db::HirDatabase = &database;
            let _ = ra_ap_hir_ty::db::HirDatabase::zalsa_register_downcaster(types);
            <dyn salsa::Database>::init_view_casters(&database);
            (database, vfs, Some(restored.contexts))
        }
        Some(Err(reason)) => {
            tracing::warn!(
                target: "cordial::cache",
                reason = %reason,
                "the saved analysis state was not used: the workspace is loaded cold"
            );
            // A run that hands its results on in a store hands no database on: a process that loads
            // cold is what it is meant to do.
            if crate::taken_up_again()
                && crate::result_store::directory_from_environment().is_none()
            {
                // Starting over would do what the process before did, and be taken up again: a
                // cycle. Said, and stopped.
                let _ = std::io::Write::write_all(
                    &mut crate::host::stderr(),
                    format!(
                        "cordial: stopped: this process was to take a run up again and could not \
                         use the state saved for it ({reason}).\n"
                    )
                    .as_bytes(),
                );
                std::process::exit(crate::MEMORY_STUCK_EXIT);
            }
            crate::host::event(format!("analysis state: not used ({reason}); loading cold"));
            (
                super::super::scan_queries::empty_database(),
                ra_ap_vfs::Vfs::default(),
                None,
            )
        }
        None => (
            super::super::scan_queries::empty_database(),
            ra_ap_vfs::Vfs::default(),
            None,
        ),
    };
    let (vfs, server) = load_workspace_into_db_with_vfs(
        workspace,
        &config.extra_env,
        &load_config,
        &mut db,
        std::mem::take(&mut vfs),
    )?;
    Ok(Loaded {
        db,
        vfs,
        server,
        kept: restored,
    })
}

/// What loading the workspace produced.
struct Loaded {
    db: RootDatabase,
    vfs: ra_ap_vfs::Vfs,
    server: Option<ra_ap_proc_macro_api::ProcMacroClient>,
    /// The cfg contexts of the saved state the database was restored from, when it was.
    kept: Option<Vec<crate::ContextCrates>>,
}
