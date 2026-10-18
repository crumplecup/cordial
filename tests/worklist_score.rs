use miette::IntoDiagnostic;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn item(id: &str, state: &str) -> Value {
    json!({
        "id": id,
        "scoring_key": "same-key",
        "area_priority": 1,
        "area": "Error handling",
        "checklist": "panics.checklist.md",
        "heading_trail": ["cordial", "panic"],
        "item_line": "same item",
        "detail_lines": [],
        "guidance": null,
        "file": "src/lib.rs",
        "state": state,
        "reopened": false
    })
}

fn worklist(items: Vec<Value>) -> miette::Result<cordial::Worklist> {
    let value = json!({
        "record": {
            "schema_version": 1,
            "cordial_version": "0.1.0",
            "config_fingerprint": "fixture",
            "generation": 1,
            "built_at_unix_secs": 0,
            "project_slug": "fixture",
            "tiers": ["all"]
        },
        "items": items,
        "cursor": null
    });
    serde_json::from_value(value).into_diagnostic()
}

#[test]
fn partial_identical_key_survival_stays_ambiguous() -> miette::Result<()> {
    cordial::init_tracing();
    let old = worklist(vec![
        item("same", "done"),
        item("same-2", "open"),
        item("same-3", "skipped"),
    ])?;
    let mut current = worklist(vec![item("same", "open"), item("same-2", "open")])?;
    let result = cordial::score(&old, &mut current);
    assert_eq!(*result.checked_off(), 1);
    assert_eq!(*result.checked_cleared(), 0);
    assert_eq!(*result.checked_reopened(), 0);
    assert!(result.cleared_claim_ids().is_empty());
    assert!(result.reopened_claim_ids().is_empty());
    assert_eq!(result.ambiguous().len(), 1);
    assert_eq!(*result.ambiguous()[0].cleared(), 1);
    assert_eq!(*result.ambiguous()[0].still_present(), 2);
    assert!(current.items().iter().all(|item| !*item.reopened()));
    Ok(())
}

#[test]
fn skipped_tier_claim_is_carried_without_scoring_or_advancing() -> miette::Result<()> {
    cordial::init_tracing();
    let mut deep = item("deep-item", "done");
    deep["checklist"] = json!("feature-warnings.checklist.md");
    deep["scoring_key"] = json!("deep-key");
    deep["pass_count"] = json!(2);
    let mut quick = item("quick-item", "open");
    quick["scoring_key"] = json!("quick-key");
    let previous = worklist(vec![deep, quick.clone()])?;
    let mut current = worklist(vec![quick])?;
    let verified = BTreeSet::from(["panics.checklist.md".to_string()]);
    let result = cordial::score_verified(&previous, &mut current, &verified);
    assert_eq!(*result.checked_off(), 0);
    assert_eq!(*result.checked_cleared(), 0);
    assert!(result.cleared_claim_ids().is_empty());
    assert_eq!(current.carry_unverified(&previous, &verified), 1);
    let deep = current
        .items()
        .iter()
        .find(|item| item.id().as_str() == "deep-item")
        .ok_or_else(|| miette::miette!("missing carried item"))?;
    assert!(*deep.unverified());
    assert_eq!(*deep.state(), cordial::ItemState::Done);
    assert_eq!(*deep.pass_count(), 2);
    assert!(
        cordial::render_stats(&current, false, None, false)
            .contains("Unverified from tiers not run: 1 item(s).")
    );
    assert!(
        cordial::render_stats(&current, true, None, true)
            .contains("deep-item  same item (on its 2nd list; unverified: tier not run)")
    );
    Ok(())
}
