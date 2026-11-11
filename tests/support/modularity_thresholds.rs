//! Small thresholds that make the modularity limits easy to cross in a fixture.

use cordial::ModularityThresholds;

pub fn test_thresholds() -> ModularityThresholds {
    ModularityThresholds::default()
        .with_file_inventory_min_lines(10)
        .with_function_inventory_min_lines(5)
        .with_function_hotspot_min_lines(5)
        .with_file_checklist_min_lines(20)
        .with_function_checklist_min_lines(15)
        .with_max_types_per_file(1)
        .with_lopsided_min_percent(60)
        .with_hierarchy_min_lines(0)
}
