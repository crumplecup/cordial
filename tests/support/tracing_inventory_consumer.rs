//! Poison loader facts to prove that enrichment consumes the shared inventory.

use cordial::{
    CordialResult, EnrichView, Etiquette, EtiquetteExplain, EtiquetteHooks, QueryBuilder,
    SourceEnrichView, SourceEnricher, SourceLoader, StaticEtiquette, TRACING_ETIQUETTE,
};

#[derive(Debug)]
pub struct InventoryConsumer {
    pub corrupt: bool,
}

impl SourceEnricher for InventoryConsumer {
    fn id(&self) -> &str {
        "function-inventory"
    }

    fn priority(&self) -> u8 {
        1
    }

    fn enrich_source(&self, view: SourceEnrichView<'_>) -> CordialResult<()> {
        let (ir, source, session) = view.into_parts();
        let module = ir.nodes_matching(
            &QueryBuilder::new()
                .has_attr(cordial::ATTR_SOURCE_TRACING_INVENTORY)
                .build(),
        )[0]
        .id();
        let inventory = ir.source_tracing_inventory(module)?.ok_or_else(|| {
            cordial::CordialError::workspace_source_facts_missing("test tracing inventory")
        })?;
        let mut payload = serde_json::to_value(inventory)?;
        if self.corrupt {
            payload["functions"][0]["context"] = serde_json::json!({});
        } else {
            payload["functions"][0]["context"]["param_names"] = serde_json::json!(["only_in_ir"]);
            payload["functions"][0]["context"]["unrecordable_params"] =
                serde_json::json!(["only_in_ir"]);
        }
        ir.set_attr(module, cordial::ATTR_SOURCE_TRACING_INVENTORY, payload)?;
        let consumer = TRACING_ETIQUETTE
            .enrichers()
            .iter()
            .find(|hook| hook.id() == self.id())
            .ok_or_else(|| {
                cordial::CordialError::workspace_source_facts_missing("function inventory hook")
            })?;
        consumer.enrich(EnrichView::new(ir, source, session))?;
        assert!(!self.corrupt, "malformed facts must fail before enrichment");
        let node = ir
            .node_by_path("work")
            .and_then(|id| ir.node(id))
            .ok_or_else(|| {
                cordial::CordialError::workspace_source_facts_missing("test function")
            })?;
        assert_eq!(node.attr("param_names"), Some(&"only_in_ir".into()));
        assert_eq!(node.attr("recipe_skip"), Some(&"only_in_ir".into()));
        Ok(())
    }
}

pub fn etiquette(corrupt: bool) -> &'static StaticEtiquette {
    let consumer = Box::leak(Box::new(InventoryConsumer { corrupt }));
    Box::leak(Box::new(StaticEtiquette::new(
        "inventory-consumer",
        "Inventory consumer test",
        EtiquetteHooks::new(
            &[&SourceLoader],
            Box::leak(Box::new([consumer as &dyn cordial::IrEnricher])),
            &[],
            &[],
            None,
            &[],
        ),
        false,
        EtiquetteExplain::new(
            "Test inventory consumption",
            "Prove shared facts own policy inputs",
            "Mutates facts before enrichment",
            "Test only",
            vec![],
        ),
    )))
}
