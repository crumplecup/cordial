//! Human-readable views over a persisted worklist.

use std::fmt::Write as _;

use tracing::instrument;

use super::{ItemState, Scorecard, Worklist, WorklistItem};

const VERBOSE_LIMIT: usize = 25;

/// Show the current item and the command that can act on it.
#[instrument(level = "trace", skip(list))]
pub fn render_open(list: &Worklist) -> String {
    let Some(item) = list.current() else {
        let skipped = list
            .items()
            .iter()
            .filter(|item| *item.state() == ItemState::Skipped)
            .count();
        let skipped_line = if skipped == 0 {
            String::new()
        } else {
            format!(" {skipped} item(s) were skipped.")
        };
        return format!(
            "The list is complete.{skipped_line}\n\nNEXT: run `cordial submit` to check your work and build a fresh list.\n"
        );
    };
    let mut output = String::new();
    let position = list.cursor().map_or(0, |index| index + 1);
    let _ = writeln!(
        output,
        "Item {position} of {} · {} · priority {} · {}{} · list {}",
        list.items().len(),
        item.id().as_str(),
        item.area_priority(),
        item.area(),
        if *item.reopened() { " · reopened" } else { "" },
        list.record().generation(),
    );
    let _ = writeln!(output, "  {}", item.item_line());
    for detail in item.detail_lines() {
        let _ = writeln!(output, "{detail}");
    }
    if let Some(guidance) = item.guidance() {
        let _ = writeln!(output, "Fix:   {guidance}");
    }
    let etiquette = item
        .checklist()
        .strip_suffix(".checklist.md")
        .unwrap_or(item.checklist())
        .replace('-', "_");
    let etiquette = if etiquette.starts_with("tracing_") {
        "tracing"
    } else {
        etiquette.as_str()
    };
    let _ = writeln!(output, "Why:   cordial explain {etiquette}");
    if let Some(file) = item.file() {
        let peers: Vec<_> = list
            .items()
            .iter()
            .enumerate()
            .filter(|(_, other)| {
                other.id() != item.id()
                    && other.file().as_deref() == Some(file.as_str())
                    && *other.state() == ItemState::Open
            })
            .map(|(index, other)| format!("{} ({})", index + 1, other.id().as_str()))
            .collect();
        if !peers.is_empty() {
            let _ = writeln!(output, "Also in {file}: {}", peers.join(", "));
        }
    }
    let _ = write!(
        output,
        "\nNEXT: fix it, then `cordial done {0}`, or set it aside with `cordial skip {0}`. New here? `cordial instructions`.\n",
        item.id().as_str(),
    );
    output
}

/// Explain how claims on the previous list fared at the new gate.
#[instrument(level = "trace", skip(previous, current, scorecard))]
pub fn render_submit(previous: &Worklist, current: &Worklist, scorecard: &Scorecard) -> String {
    let mut output = String::new();
    let _ = writeln!(
        output,
        "Gate: {} open items remain ({} at the last run).",
        current.items().len(),
        previous.items().len()
    );
    let ambiguous_claims = scorecard
        .checked_off()
        .saturating_sub(*scorecard.checked_cleared() + *scorecard.checked_reopened());
    let _ = writeln!(
        output,
        "Of {} items checked off: {} cleared, {} still open{}.",
        scorecard.checked_off(),
        scorecard.checked_cleared(),
        scorecard.checked_reopened(),
        if ambiguous_claims == 0 {
            String::new()
        } else {
            format!(", {ambiguous_claims} in ambiguous groups")
        }
    );
    if !scorecard.cleared_claim_ids().is_empty() {
        let _ = writeln!(
            output,
            "Cleared claims: {}.",
            scorecard.cleared_claim_ids().join(", ")
        );
    }
    if !scorecard.reopened_claim_ids().is_empty() {
        let _ = writeln!(
            output,
            "Reopened claims: {}.",
            scorecard.reopened_claim_ids().join(", ")
        );
    }
    for group in scorecard.ambiguous() {
        let _ = writeln!(
            output,
            "Ambiguous identical items {}: {} cleared, {} still present; individual ids cannot be assigned.",
            group.ids().join(", "),
            group.cleared(),
            group.still_present()
        );
    }
    let _ = writeln!(
        output,
        "{}cleared without being checked off: {}.",
        if scorecard.ambiguous().is_empty() {
            "C"
        } else {
            "Definitely c"
        },
        scorecard.cleared_without_claim()
    );
    if *scorecard.skipped_carried() > 0 {
        let _ = writeln!(
            output,
            "Skipped: {}, carried to the new list.",
            scorecard.skipped_carried()
        );
    }
    let _ = writeln!(output, "New since the last run: {}.", scorecard.new_items());
    output.push_str("\n Priority  Area                         Was  Now\n");
    for area in scorecard.per_area() {
        let _ = writeln!(
            output,
            " {:>7}  {:<28} {:>3}  {:>3}",
            area.priority(),
            area.area(),
            area.was(),
            area.now()
        );
    }
    let _ = write!(
        output,
        "\nNEXT: new list written (list {}), reopened items first. `cordial open` shows the first.\n",
        current.record().generation()
    );
    output
}

/// Summarize the current list, optionally showing individual items.
#[instrument(level = "trace", skip(list))]
pub fn render_stats(list: &Worklist, verbose: bool, area: Option<&str>, all: bool) -> String {
    let mut output = String::new();
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
    let _ = writeln!(
        output,
        "{} open, {checked} checked off (unverified), {skipped} skipped. List {}.",
        list.open_count(),
        list.record().generation()
    );
    let selected: Vec<&WorklistItem> = list
        .items()
        .iter()
        .filter(|item| area.is_none_or(|name| item.area().eq_ignore_ascii_case(name)))
        .collect();
    if let Some(name) = area {
        if selected.is_empty() {
            let _ = write!(output, "\nNo area named {name}.\n\nNEXT: cordial stats\n");
            return output;
        }
    }
    output.push('\n');
    if verbose {
        render_verbose(&mut output, &selected, list, all);
    } else {
        render_area_table(&mut output, &selected);
    }
    if let Some(index) = list.cursor() {
        let _ = writeln!(
            output,
            "\nNEXT: current item is {} of {}. `cordial peek` shows it.",
            index + 1,
            list.items().len()
        );
    } else if list.items().is_empty()
        || list
            .items()
            .iter()
            .all(|item| *item.state() != ItemState::Open)
    {
        output.push_str("\nNEXT: cordial submit\n");
    } else {
        output.push_str("\nNEXT: cordial open\n");
    }
    output
}

#[instrument(level = "trace", skip(output, items))]
fn render_area_table(output: &mut String, items: &[&WorklistItem]) {
    output.push_str(" Priority  Area                         Open  Checked  Skipped\n");
    let mut areas: Vec<(&str, u8)> = Vec::new();
    for item in items {
        let key = (item.area().as_str(), *item.area_priority());
        if !areas.contains(&key) {
            areas.push(key);
        }
    }
    for (name, priority) in areas {
        let in_area = items.iter().filter(|item| item.area() == name);
        let open = in_area
            .clone()
            .filter(|item| *item.state() != ItemState::Done)
            .count();
        let checked = in_area
            .clone()
            .filter(|item| *item.state() == ItemState::Done)
            .count();
        let skipped = in_area
            .filter(|item| *item.state() == ItemState::Skipped)
            .count();
        let _ = writeln!(
            output,
            " {priority:>7}  {name:<28} {open:>4}  {checked:>7}  {skipped:>7}"
        );
    }
}

#[instrument(level = "trace", skip(output, items, list))]
fn render_verbose(output: &mut String, items: &[&WorklistItem], list: &Worklist, all: bool) {
    let shown = if all {
        items.len()
    } else {
        items.len().min(VERBOSE_LIMIT)
    };
    let mut previous_area = None;
    for item in items.iter().take(shown) {
        if previous_area != Some(item.area().as_str()) {
            if previous_area.is_some() {
                output.push('\n');
            }
            let _ = writeln!(output, "{}", item.area());
            previous_area = Some(item.area().as_str());
        }
        let mark = if list
            .current()
            .is_some_and(|current| current.id() == item.id())
        {
            '>'
        } else {
            match item.state() {
                ItemState::Open => ' ',
                ItemState::Done => 'x',
                ItemState::Skipped => '-',
            }
        };
        let _ = writeln!(
            output,
            "  [{mark}] {}  {}",
            item.id().as_str(),
            item.item_line()
        );
    }
    if shown < items.len() {
        let _ = writeln!(output, "Showing {shown} of {} items.", items.len());
        output.push_str("Use `cordial stats --area <title>` or `--all` for more.\n");
    }
}
