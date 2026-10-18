//! Machine-readable views over the same worklist and scorecard as text output.

use serde_json::{Value, json};
use tracing::instrument;

use crate::StoreLayout;

use super::{ClaimChange, ItemState, Scorecard, SoftStaleness, Worklist};

const VERSION: u32 = 1;
const VERBOSE_LIMIT: usize = 25;

#[instrument(level = "debug", skip(store, list, stale))]
pub(crate) fn json_open(
    command: &str,
    store: &StoreLayout,
    list: &Worklist,
    stale: &SoftStaleness,
) -> Value {
    let current = list.current().map(|item| {
        let mut value = json!(item);
        value["stop"] = json!(item.needs_user(*list.max_passes()));
        value["also_in_same_file"] = json!(
            list.items()
                .iter()
                .filter(|other| {
                    other.id() != item.id()
                        && *other.state() == ItemState::Open
                        && other.file() == item.file()
                        && item.file().is_some()
                })
                .map(|other| other.id().as_str())
                .collect::<Vec<_>>()
        );
        value
    });
    let next = list.current().map_or_else(
        || "cordial submit".to_string(),
        |item| {
            if item.needs_user(*list.max_passes()) {
                "Tell the user what blocked this item".to_string()
            } else {
                format!("cordial done {}", item.id().as_str())
            }
        },
    );
    json!({
        "version": VERSION,
        "command": command,
        "list_generation": list.record().generation(),
        "store_path": store.worklist_path(),
        "tiers": list.record().tiers(),
        "built_at_unix_secs": list.record().built_at_unix_secs(),
        "cursor": {"index": list.cursor().map(|index| index + 1), "total_open": list.open_count()},
        "current": current,
        "next": next,
        "soft_stale": {"age_secs": stale.age_secs(), "changed_files": stale.changed_files(), "is_soft_stale": stale.is_soft_stale()},
    })
}

#[instrument(level = "debug", skip(store, list, stale, changes))]
pub(crate) fn json_claim(
    command: &str,
    store: &StoreLayout,
    list: &Worklist,
    stale: &SoftStaleness,
    changes: &[ClaimChange],
) -> Value {
    let mut value = json_open(command, store, list, stale);
    value["checked_off"] = json!(
        changes
            .iter()
            .map(|change| json!({
                "id": change.id(),
                "was": change.was(),
                "now": change.now(),
            }))
            .collect::<Vec<_>>()
    );
    value
}

#[instrument(level = "trace", skip(store, list, stale))]
pub(crate) fn json_stats(
    store: &StoreLayout,
    list: &Worklist,
    stale: &SoftStaleness,
    verbose: bool,
    area: Option<&str>,
    all: bool,
) -> Value {
    let selected: Vec<_> = list
        .items()
        .iter()
        .filter(|item| area.is_none_or(|name| item.area().eq_ignore_ascii_case(name)))
        .collect();
    let items = verbose.then(|| {
        selected
            .iter()
            .take(if all { usize::MAX } else { VERBOSE_LIMIT })
            .map(|item| {
                let mut value = json!(item);
                value["stop"] = json!(item.needs_user(*list.max_passes()));
                value
            })
            .collect::<Vec<_>>()
    });
    let checked = list
        .items()
        .iter()
        .filter(|item| *item.state() == ItemState::Done)
        .count();
    let skipped = list
        .items()
        .iter()
        .filter(|item| *item.state() == ItemState::Skipped)
        .count();
    let mut areas = std::collections::BTreeMap::<(u8, String), (usize, usize, usize)>::new();
    for item in list.items() {
        let counts = areas
            .entry((*item.area_priority(), item.area().clone()))
            .or_default();
        match item.state() {
            ItemState::Open => counts.0 += 1,
            ItemState::Done => counts.1 += 1,
            ItemState::Skipped => counts.2 += 1,
        }
    }
    let per_area: Vec<_> = areas.into_iter().map(|((priority, area), (open, checked, skipped))| {
        json!({"priority": priority, "area": area, "open": open, "checked": checked, "skipped": skipped})
    }).collect();
    json!({
        "version": VERSION,
        "command": "stats",
        "list_generation": list.record().generation(),
        "store_path": store.worklist_path(),
        "tiers": list.record().tiers(),
        "counts": {"open": list.open_count(), "checked": checked, "skipped": skipped, "total": list.items().len()},
        "per_area": per_area,
        "soft_stale": {"age_secs": stale.age_secs(), "changed_files": stale.changed_files(), "is_soft_stale": stale.is_soft_stale()},
        "validity": {"hard_invalid": false},
        "items": items,
        "shown": items.as_ref().map(Vec::len),
        "matching": selected.len(),
        "next": "cordial open",
    })
}

#[instrument(level = "trace", skip(store, previous, current, scorecard))]
pub(crate) fn json_submit(
    store: &StoreLayout,
    previous: &Worklist,
    current: &Worklist,
    scorecard: &Scorecard,
) -> Value {
    let outcomes: Vec<_> = scorecard
        .cleared_claim_ids()
        .iter()
        .map(|id| json!({"id": id, "outcome": "cleared"}))
        .chain(
            scorecard
                .reopened_claim_ids()
                .iter()
                .map(|id| json!({"id": id, "outcome": "reopened"})),
        )
        .collect();
    let needs_user: Vec<_> = current.items().iter()
        .filter(|item| item.needs_user(*current.max_passes()))
        .map(|item| json!({"id": item.id(), "pass_count": item.pass_count(), "skip_reasons": item.skip_reasons()}))
        .collect();
    let per_area: Vec<_> = scorecard.per_area().iter().map(|area| json!({
        "priority": area.priority(), "area": area.area(), "was": area.was(), "now": area.now(),
    })).collect();
    let ambiguous: Vec<_> = scorecard.ambiguous().iter().map(|group| json!({
        "ids": group.ids(), "cleared": group.cleared(), "still_present": group.still_present(),
    })).collect();
    json!({
        "version": VERSION,
        "command": "submit",
        "list_generation": current.record().generation(),
        "previous_generation": previous.record().generation(),
        "store_path": store.worklist_path(),
        "tiers": current.record().tiers(),
        "gate": {"open_before": previous.open_count(), "open_now": current.open_count(), "tiers_run": current.record().tiers()},
        "scorecard": {
            "checked_off": {"total": scorecard.checked_off(), "cleared": scorecard.checked_cleared(), "reopened": scorecard.checked_reopened()},
            "cleared_without_claim": scorecard.cleared_without_claim(),
            "skipped_carried": scorecard.skipped_carried(),
            "new_items": scorecard.new_items(),
            "remaining_untouched": scorecard.remaining_untouched(),
            "ambiguous": ambiguous,
        },
        "outcomes": outcomes,
        "needs_user": needs_user,
        "per_area": per_area,
        "next": if needs_user.is_empty() { "cordial open" } else { "Tell the user about blocked items" },
    })
}

#[instrument(level = "trace", skip(store, list))]
pub(crate) fn json_status(
    command: &str,
    store: &StoreLayout,
    list: Option<&Worklist>,
    reason: &str,
) -> Value {
    json!({
        "version": VERSION,
        "command": command,
        "list_generation": list.map(|list| list.record().generation()),
        "store_path": store.worklist_path(),
        "status": reason,
        "next": "cordial open",
    })
}

#[instrument(level = "trace", skip(store, message))]
pub(crate) fn json_error(command: &str, store: &StoreLayout, message: &str) -> Value {
    let generation = Worklist::read(store)
        .ok()
        .flatten()
        .map(|list| *list.record().generation());
    let kind = if message.contains("No item ") {
        "UnknownId"
    } else {
        "CommandFailed"
    };
    json!({
        "version": VERSION,
        "command": command,
        "list_generation": generation,
        "store_path": store.worklist_path(),
        "error": {"kind": kind, "message": message},
        "next": "cordial stats --verbose --all",
    })
}
