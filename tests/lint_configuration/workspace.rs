//! Workspace hooks share the central policy, including findings without locations.

use std::{fs, sync::LazyLock};

use cordial::{
    CordialResult, Disposition, EtiquetteExplain, EtiquetteHooks, Finding, FindingSink, IrAnchor,
    NodeAnchor, NodeId, Rule, RunAll, Session, SessionBuilder, SourceLoader, StaticEtiquette,
    TomlConfigured, WorkspaceAssessView, WorkspaceAssessor,
};
use miette::IntoDiagnostic;

struct WorkspaceRule;

impl TomlConfigured for WorkspaceRule {
    fn configuration_key(&self) -> &str {
        "workspace-policy"
    }
}

impl Rule for WorkspaceRule {
    fn id(&self) -> &str {
        "WORKSPACE-001"
    }
    fn description(&self) -> &str {
        "Workspace test finding"
    }
}

struct WorkspaceFinding {
    anchor: NodeAnchor,
    file: Option<String>,
}

impl Finding for WorkspaceFinding {
    fn rule(&self) -> &dyn Rule {
        &WorkspaceRule
    }
    fn disposition(&self) -> Disposition {
        Disposition::Open
    }
    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }
    fn emit(&self, sink: &mut dyn FindingSink) {
        sink.field("crate", &"policy-fixture");
        if let Some(file) = &self.file {
            sink.field("file", file);
        }
    }
}

struct WorkspaceCheck;

impl WorkspaceAssessor for WorkspaceCheck {
    fn id(&self) -> &str {
        "workspace-policy"
    }
    fn assess(&self, _: WorkspaceAssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>> {
        Ok([Some("src/lib.rs"), None]
            .into_iter()
            .map(|file| {
                Box::new(WorkspaceFinding {
                    anchor: NodeAnchor::new(NodeId::new(u32::MAX)),
                    file: file.map(str::to_owned),
                }) as Box<dyn Finding>
            })
            .collect())
    }
}

static ETIQUETTE: LazyLock<StaticEtiquette> = LazyLock::new(|| {
    StaticEtiquette::new(
        "workspace-policy",
        "Workspace policy",
        EtiquetteHooks::new(
            &[&SourceLoader],
            &[],
            &[],
            &[],
            Some(&[&WorkspaceCheck]),
            &[],
        ),
        false,
        EtiquetteExplain::new("Test", "Test", "Test", "Shared policy", vec![]),
    )
});

#[test]
fn workspace_findings_use_crate_paths_but_unknown_locations_keep_global_policy()
-> miette::Result<()> {
    cordial::init_tracing();
    let project = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir(project.path().join("src")).into_diagnostic()?;
    fs::write(project.path().join("Cargo.toml"),
        "[package]\nname = \"policy-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n").into_diagnostic()?;
    fs::write(project.path().join("src/lib.rs"), "pub fn ready() {}\n").into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(project.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&*ETIQUETTE)
        .build();
    assert_eq!(
        session.run(&RunAll).into_diagnostic()?.findings().count(),
        2
    );
    fs::write(
        project.path().join("cordial.toml"),
        "[[lints.workspace-policy.paths]]\npath = \"src\"\nenabled = false\n",
    )
    .into_diagnostic()?;
    let result = session.run(&RunAll).into_diagnostic()?;
    assert_eq!(result.findings().count(), 1);
    let mut sink = cordial::MapFindingSink::default();
    for finding in result.findings() {
        finding.emit(&mut sink);
    }
    assert!(!sink.fields().iter().any(|(key, _)| key == "file"));
    fs::write(
        project.path().join("cordial.toml"),
        "[lints.workspace-policy.rules]\nWORKSPACE-001 = false\n",
    )
    .into_diagnostic()?;
    assert_eq!(
        session.run(&RunAll).into_diagnostic()?.findings().count(),
        0
    );
    Ok(())
}
