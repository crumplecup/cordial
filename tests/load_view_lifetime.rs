//! Loader bundles end at each crate's enrichment boundary, while probes use its IR.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};

use cordial::{
    CordialError, CordialResult, EnrichView, EtiquetteExplain, EtiquetteHooks, IrEnricher,
    LoadContext, LoadView, Loader, Marker, Probe, ProbeView, RunAll, Session, SessionBuilder,
    SourceLoader, StaticEtiquette,
};

const PAYLOAD_BYTES: usize = 256 * 1024;

#[derive(Default)]
struct Observations {
    bundles: BTreeMap<(String, &'static str), Weak<Vec<u8>>>,
    hooks: Vec<String>,
}

static OBSERVATIONS: LazyLock<Mutex<Observations>> = LazyLock::new(Mutex::default);

struct Input(&'static str);

static FIRST: Input = Input("first-bundle");
static SECOND: Input = Input("second-bundle");
static LOADERS: &[&dyn Loader] = &[&SourceLoader, &FIRST, &SECOND];
static ENRICHERS: &[&dyn IrEnricher] = &[&FIRST, &SECOND];

struct Bundle {
    crate_name: String,
    loader: &'static str,
    payload: Arc<Vec<u8>>,
}

impl LoadView for Bundle {
    fn loader_id(&self) -> &str {
        self.loader
    }

    fn crate_name(&self) -> &str {
        &self.crate_name
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Loader for Input {
    fn id(&self) -> &str {
        self.0
    }

    fn load(&self, view: LoadContext<'_>) -> CordialResult<Box<dyn LoadView>> {
        let payload = Arc::new(vec![42; PAYLOAD_BYTES]);
        let crate_name = view.target().crate_name().clone();
        OBSERVATIONS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .bundles
            .insert((crate_name.clone(), self.0), Arc::downgrade(&payload));
        Ok(Box::new(Bundle {
            crate_name,
            loader: self.0,
            payload,
        }))
    }
}

impl IrEnricher for Input {
    fn id(&self) -> &str {
        self.0
    }

    fn required_loader(&self) -> &str {
        self.0
    }

    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let (ir, load, _) = view.into_parts();
        let bundle = load
            .as_any()
            .downcast_ref::<Bundle>()
            .ok_or_else(|| CordialError::workspace_source_facts_missing("test loader bundle"))?;
        let mut observations = OBSERVATIONS.lock().unwrap_or_else(PoisonError::into_inner);
        for ((crate_name, _), weak) in &observations.bundles {
            if crate_name == ir.crate_name() {
                assert!(
                    weak.upgrade().is_some(),
                    "all inputs survive every enricher"
                );
            }
        }
        observations
            .hooks
            .push(format!("{}:{}", ir.crate_name(), self.0));
        assert_eq!(bundle.payload.len(), PAYLOAD_BYTES);
        ir.set_attr(ir.root()?, self.0, serde_json::json!(bundle.payload.len()))?;
        Ok(())
    }
}

struct Observer;

impl Probe for Observer {
    fn id(&self) -> &str {
        "bundle-lifetime"
    }

    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir();
        let mut observations = OBSERVATIONS.lock().unwrap_or_else(PoisonError::into_inner);
        for ((crate_name, loader), weak) in &observations.bundles {
            assert_eq!(
                weak.upgrade().is_some(),
                crate_name.as_str() > ir.crate_name(),
                "{crate_name}:{loader}: only later crates retain loader data"
            );
        }
        let root = ir
            .node(ir.root()?)
            .ok_or_else(|| CordialError::workspace_source_facts_missing("test IR root"))?;
        for key in [FIRST.0, SECOND.0] {
            assert_eq!(root.attr(key), Some(&serde_json::json!(PAYLOAD_BYTES)));
        }
        observations
            .hooks
            .push(format!("{}:probe", ir.crate_name()));
        Ok(Vec::new())
    }
}

static ETIQUETTE: LazyLock<StaticEtiquette> = LazyLock::new(|| {
    StaticEtiquette::new(
        "bundle-lifetime",
        "Loader bundle lifetime fixture",
        EtiquetteHooks::new(LOADERS, ENRICHERS, &[&Observer], &[], None, &[]),
        false,
        EtiquetteExplain::new("Test", "Test", "Loader release", "Test only", vec![]),
    )
});

#[test]
fn cold_and_cached_runs_release_each_crates_inputs_before_probing()
-> Result<(), Box<dyn std::error::Error>> {
    let source = tempfile::tempdir()?;
    let store = tempfile::tempdir()?;
    std::fs::write(
        source.path().join("Cargo.toml"),
        "[workspace]\nmembers=['alpha','beta']\nresolver='3'\n",
    )?;
    for (name, dependencies, code) in [
        ("alpha", "", "pub fn leaf() -> u32 { 42 }"),
        (
            "beta",
            "[dependencies]\nalpha={path='../alpha'}\n",
            "pub fn caller() -> u32 { alpha::leaf() }",
        ),
    ] {
        let root = source.path().join(name);
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2024'\n{dependencies}"),
        )?;
        std::fs::write(root.join("src/lib.rs"), code)?;
    }
    let session = SessionBuilder::new(source.path())
        .with_store_root(store.path())
        .register(&*ETIQUETTE)
        .build();
    for _ in 0..2 {
        *OBSERVATIONS.lock().unwrap_or_else(PoisonError::into_inner) = Observations::default();
        session.run(&RunAll)?;
        let observations = OBSERVATIONS.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(observations.bundles.len(), 4);
        assert!(
            observations
                .bundles
                .values()
                .all(|weak| weak.upgrade().is_none())
        );
        assert_eq!(
            observations.hooks,
            [
                "alpha:first-bundle",
                "alpha:second-bundle",
                "alpha:probe",
                "beta:first-bundle",
                "beta:second-bundle",
                "beta:probe",
            ]
        );
    }
    Ok(())
}
