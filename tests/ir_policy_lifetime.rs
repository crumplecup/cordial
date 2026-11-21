//! Whole-crate decoded policy evidence ends at the enrichment boundary.
#![cfg(feature = "allows")]

use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};

use cordial::{
    CordialResult, EnrichView, EtiquetteExplain, EtiquetteHooks, IrEnricher, Marker, Probe,
    ProbeView, RunAll, Session, SessionBuilder, SourceLoader, SourcePolicyInventory,
    StaticEtiquette,
};

struct Observer(Mutex<Option<Weak<SourcePolicyInventory>>>);

static OBSERVER: Observer = Observer(Mutex::new(None));
static ENRICHERS: &[&dyn IrEnricher] = &[&OBSERVER];
static PROBES: &[&dyn Probe] = &[&OBSERVER];

impl Observer {
    fn assert_released(&self) {
        let weak = self.0.lock().unwrap_or_else(PoisonError::into_inner).take();
        assert!(weak.is_some_and(|weak| weak.upgrade().is_none()));
    }
}

impl IrEnricher for Observer {
    fn id(&self) -> &str {
        "policy-lifetime"
    }

    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir();
        let inventory = ir.shared_source_policy_inventory()?.ok_or_else(|| {
            cordial::CordialError::workspace_source_facts_missing("test policy inventory")
        })?;
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::downgrade(&inventory));
        Ok(())
    }
}

impl Probe for Observer {
    fn id(&self) -> &str {
        "policy-lifetime"
    }

    fn probe(&self, _: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        self.assert_released();
        Ok(Vec::new())
    }
}

static ETIQUETTE: LazyLock<StaticEtiquette> = LazyLock::new(|| {
    StaticEtiquette::new(
        "policy-lifetime",
        "Policy lifetime fixture",
        EtiquetteHooks::new(&[&SourceLoader], ENRICHERS, PROBES, &[], None, &[]),
        false,
        EtiquetteExplain::new(
            "Test",
            "Test",
            "Decoded inventory release",
            "Test only",
            vec![],
        ),
    )
});

#[test]
fn normal_loading_releases_shared_inventories_after_enrichment()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let store = tempfile::tempdir()?;
    std::fs::create_dir(root.path().join("src"))?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='policy-lifetime'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )?;
    std::fs::write(root.path().join("src/lib.rs"), "pub fn work() {}\n")?;
    let session = SessionBuilder::new(root.path())
        .with_store_root(store.path())
        .register(&*ETIQUETTE)
        .build();
    session.run(&RunAll)?;

    Ok(())
}
