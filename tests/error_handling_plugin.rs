#[cfg(feature = "error_sites")]
use cordial::{ERROR_IR_ENRICHERS, ErrorHandlingLayers, error_ir_enricher_ids};
use cordial::{
    Plugin, PluginCategory, STANDARD_ERROR_HANDLING, quality_plugins,
    standard_error_handling_etiquettes,
};

#[test]
fn standard_error_handling_registers_full_stack() {
    assert_eq!(STANDARD_ERROR_HANDLING.id(), "error-handling");
    assert_eq!(
        STANDARD_ERROR_HANDLING.category(),
        PluginCategory::ErrorHandling
    );

    let etiquettes = standard_error_handling_etiquettes();
    assert!(!etiquettes.is_empty());
    #[cfg(feature = "error_sites")]
    assert!(
        etiquettes
            .iter()
            .any(|etiquette| etiquette.id() == "error_sites")
    );
    #[cfg(feature = "panics")]
    assert!(
        etiquettes
            .iter()
            .any(|etiquette| etiquette.id() == "panics"),
        "panicking APIs belong on the error-handling plugin"
    );

    let plugin_etiquettes = STANDARD_ERROR_HANDLING.etiquettes();
    assert_eq!(plugin_etiquettes.len(), etiquettes.len());
}

#[test]
fn quality_plugins_register_error_handling_once() {
    let plugins = quality_plugins();
    let error_handling = plugins
        .iter()
        .filter(|plugin| plugin.id() == "error-handling")
        .count();
    assert_eq!(error_handling, 1);
    assert!(
        !plugins.iter().any(|plugin| plugin.id() == "panics"),
        "panics must not also wrap as a standalone quality plugin"
    );
}

#[cfg(feature = "error_sites")]
#[test]
fn error_ir_enrichers_include_full_stack_layers() {
    assert!(!ERROR_IR_ENRICHERS.is_empty());
    assert!(
        ERROR_IR_ENRICHERS
            .iter()
            .any(|enricher| enricher.id() == "error-ir-scan")
    );
    assert!(
        ERROR_IR_ENRICHERS
            .iter()
            .any(|enricher| enricher.id() == "error-flow")
    );

    let ids = error_ir_enricher_ids(ErrorHandlingLayers::FULL);
    assert!(ids.contains(&"error-ir-scan"));
    assert!(ids.contains(&"error-flow"));
    assert!(ids.contains(&"foreign-error-attenuation-inventory"));
}

#[cfg(feature = "foreign_error_attenuation")]
#[test]
fn full_quality_feature_includes_attenuation_layer() {
    let ids: Vec<&str> = standard_error_handling_etiquettes()
        .into_iter()
        .map(|etiquette| etiquette.id())
        .collect();
    assert!(ids.contains(&"foreign_error_attenuation"));
}
