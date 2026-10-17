//! Human-readable views over a persisted worklist.

use std::fmt::Write as _;

use tracing::instrument;

use super::{ItemState, Worklist, WorklistItem};

const VERBOSE_LIMIT: usize = 25;

/// Show the current item and the command that can act on it.
#[instrument(level = "trace", skip(list))]
pub fn render_open(list: &Worklist) -> String {
    let Some(item) = list.current() else {
        return format!(
            "List {} is complete.\n\nNEXT: cordial submit\n",
            list.record().generation()
        );
    };
    let mut output = String::new();
    let position = list.cursor().map_or(0, |index| index + 1);
    let _ = writeln!(
        output,
        "Item {position} of {} · {} · priority {} · {}",
        list.items().len(),
        item.id().as_str(),
        item.area_priority(),
        item.area()
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
        "\nNEXT: fix it, then `cordial done {}`. New here? `cordial instructions`.\n",
        item.id().as_str()
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
