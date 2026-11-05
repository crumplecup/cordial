//! Cargo context is part of shared semantic evidence and reload boundaries.

mod support {
    pub mod semantic_workspace;
}
use cordial::{SemanticCargoOptions, SemanticFeatures, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use support::semantic_workspace::{assert_error, function, project, snapshot, specimen};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn cargo_context_supplies_generated_types_cfg_dependencies_and_proc_macros() -> Result<()> {
    cordial::init_tracing();
    let root = specimen()?;
    let options = SemanticCargoOptions::default()
        .with_environment([("CONTEXT_ERROR".into(), Some("std::io::Error".into()))].into());
    let mut session = SourceDatabase::load_with(root.path(), &options).into_diagnostic()?;
    let graph = project(&mut session, root.path())?;
    assert_error(function(&graph, "generated")?.output(), "Error");
    assert_error(
        function(&graph, "feature_contract")?.output(),
        "ParseIntError",
    );
    for name in ["macro_call", "dependency_call"] {
        let caller = function(&graph, name)?;
        assert_eq!(caller.calls().len(), 1, "{name}: {caller:?}");
        assert_error(caller.calls()[0].output(), "Error");
    }
    let initial = snapshot(&graph)?;
    assert_workspace_context(&initial, root.path())?;
    assert_context_guards(&mut session, root.path(), &initial)?;

    let options = options
        .with_environment(
            [(
                "CONTEXT_ERROR".into(),
                Some("std::num::ParseIntError".into()),
            )]
            .into(),
        )
        .with_features(SemanticFeatures::Selected {
            features: vec!["alternate".into()],
            no_default_features: true,
        });
    session.reload(root.path(), &options).into_diagnostic()?;
    let changed = project(&mut session, root.path())?;
    assert_error(function(&changed, "generated")?.output(), "ParseIntError");
    assert_error(function(&changed, "feature_contract")?.output(), "Error");
    let current = snapshot(&changed)?;
    assert_ne!(
        current.workspace().identity(),
        initial.workspace().identity()
    );
    assert_ne!(
        current.workspace().environment_digest(),
        initial.workspace().environment_digest()
    );
    assert!(current.revision() > initial.revision());
    // Reload cannot mutate evidence already emitted to consumers.
    assert_eq!(snapshot(&graph)?, initial);
    assert_failed_reload_preserves_session(&mut session, root.path(), &options, &current)?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_context_guards(
    session: &mut SourceDatabase,
    root: &std::path::Path,
    initial: &cordial::SemanticSnapshot,
) -> Result<()> {
    for name in ["Cargo.toml", "build.rs", "macros/src/lib.rs"] {
        assert!(
            session
                .change_file(&root.join(name), "// not a source-only edit".into())
                .is_err()
        );
    }
    assert_eq!(&snapshot(&project(session, root)?)?, initial);
    let path = root.join(".cargo/config.toml");
    std::fs::create_dir_all(root.join(".cargo")).into_diagnostic()?;
    std::fs::write(&path, "[build]\n").into_diagnostic()?;
    assert!(
        project(session, root).is_err(),
        "new Cargo configuration must require reload"
    );
    std::fs::remove_file(path).into_diagnostic()?;
    let source = root.join("src/lib.rs");
    let text = std::fs::read_to_string(&source).into_diagnostic()?;
    session
        .change_file(&source, format!("{text}\npub fn virtual_only()->u8{{3}}\n"))
        .into_diagnostic()?;
    let edited = project(session, root)?;
    function(&edited, "virtual_only")?;
    assert_eq!(snapshot(&edited)?.workspace(), initial.workspace());
    assert_eq!(std::fs::read_to_string(source).into_diagnostic()?, text);
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_failed_reload_preserves_session(
    session: &mut SourceDatabase,
    root: &std::path::Path,
    options: &SemanticCargoOptions,
    current: &cordial::SemanticSnapshot,
) -> Result<()> {
    let manifest = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).into_diagnostic()?;
    std::fs::write(&manifest, "this is invalid TOML").into_diagnostic()?;
    assert!(session.reload(root, options).is_err());
    std::fs::write(manifest, text).into_diagnostic()?;
    assert_eq!(&snapshot(&project(session, root)?)?, current);
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_workspace_context(
    initial: &cordial::SemanticSnapshot,
    root: &std::path::Path,
) -> Result<()> {
    assert!(
        initial.workspace().gaps().is_empty(),
        "{:?}",
        initial.workspace().gaps()
    );
    assert!(initial.workspace().proc_macro_server().is_some());
    assert!(initial.workspace().sysroot().is_some());
    assert!(
        initial
            .workspace()
            .crates()
            .iter()
            .any(|krate| *krate.proc_macro())
    );
    assert!(
        initial
            .workspace()
            .crates()
            .iter()
            .any(|krate| krate.cfg().iter().any(|cfg| cfg == "build_ready"))
    );
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "macros/Cargo.toml",
        "dependency/Cargo.toml",
    ] {
        assert!(
            initial
                .workspace()
                .configuration_inputs()
                .iter()
                .any(|input| input.path() == &root.join(name) && input.digest().is_some())
        );
    }
    let restored: cordial::SemanticSnapshot =
        serde_json::from_value(serde_json::to_value(&initial).into_diagnostic()?)
            .into_diagnostic()?;
    assert_eq!(initial, &restored);
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn failed_build_outputs_stay_unavailable_in_shared_evidence() -> Result<()> {
    cordial::init_tracing();
    let root = specimen()?;
    let options =
        SemanticCargoOptions::default().with_environment([("CONTEXT_ERROR".into(), None)].into());
    let mut session = SourceDatabase::load_with(root.path(), &options).into_diagnostic()?;
    let graph = project(&mut session, root.path())?;
    assert!(
        snapshot(&graph)?
            .workspace()
            .gaps()
            .contains(&cordial::SemanticContextGap::BuildScripts)
    );
    let generated = function(&graph, "generated")?;
    assert!(*generated.incomplete());
    assert_eq!(
        generated.result_recording(),
        &cordial::SemanticResultRecording::Result {
            display: cordial::SemanticCapability::Unavailable,
            debug: cordial::SemanticCapability::Unavailable,
        }
    );
    Ok(())
}
