use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::{CordialResult, QualityAreaSummary, StoreLayout};

use super::WorklistItem;

use tracing::instrument;
/// Supplies worklist items for one quality area.
pub trait ItemSource {
    /// Read items in checklist order.
    fn items(&self, area: &QualityAreaSummary) -> CordialResult<Vec<WorklistItem>>;
}

/// Reads the checklist markdown that the quality reporters wrote.
#[derive(Debug, Clone)]
pub struct ChecklistSource {
    findings_dir: PathBuf,
    fresh_since: Option<SystemTime>,
}

impl ChecklistSource {
    /// Read checklists under the given store's `findings/` directory.
    #[instrument(level = "debug", skip(store))]
    pub fn new(store: &StoreLayout) -> Self {
        Self {
            findings_dir: store.findings_dir(),
            fresh_since: None,
        }
    }

    /// Read checklists from an explicit directory, useful for library callers.
    #[instrument(level = "debug", skip(path))]
    pub fn from_findings_dir(path: impl Into<PathBuf>) -> Self {
        Self {
            findings_dir: path.into(),
            fresh_since: None,
        }
    }

    /// Read only checklists written by the current gate run.
    #[instrument(level = "trace", skip(self, started))]
    pub fn fresh_since(mut self, started: SystemTime) -> Self {
        self.fresh_since = Some(started);
        self
    }
}

impl ItemSource for ChecklistSource {
    #[instrument(level = "trace", skip(self, area))]
    fn items(&self, area: &QualityAreaSummary) -> CordialResult<Vec<WorklistItem>> {
        let mut items = Vec::new();
        for checklist in area.checklists() {
            let path = self.findings_dir.join(checklist.name());
            if self.fresh_since.is_some_and(|started| {
                fs::metadata(&path)
                    .and_then(|metadata| metadata.modified())
                    .is_ok_and(|modified| modified < started)
            }) {
                continue;
            }
            let body = match fs::read_to_string(&path) {
                Ok(body) => body,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            parse_checklist(area, checklist, &body, &mut items);
        }
        Ok(items)
    }
}

#[instrument(level = "debug", skip(area, checklist, items))]
fn parse_checklist(
    area: &QualityAreaSummary,
    checklist: &crate::QualityChecklist,
    body: &str,
    items: &mut Vec<WorklistItem>,
) {
    let mut headings: Vec<(usize, String, Option<String>)> = Vec::new();
    let mut rule = "";
    let mut active_item = None;
    for line in body.lines() {
        if let Some((level, heading)) = heading(line) {
            headings.retain(|(prior, _, _)| *prior < level);
            if level >= 2 {
                headings.push((level, heading.to_string(), None));
            }
            if level == 2 {
                rule = "";
            } else if level == 3 {
                rule = heading;
            }
            active_item = None;
            continue;
        }
        if let Some(guidance) = italic_paragraph(line)
            && let Some((_, _, slot)) = headings.last_mut()
        {
            *slot = Some(guidance.to_string());
        }
        if let Some(item_line) = line.strip_prefix("- [ ] ") {
            active_item = None;
            if checklist.includes_rule(rule) {
                let trail = headings.iter().map(|(_, title, _)| title.clone()).collect();
                let guidance = headings
                    .iter()
                    .rev()
                    .find_map(|(_, _, guidance)| guidance.clone());
                let file = file_hint(item_line);
                items.push(WorklistItem::from_checklist(
                    area.priority(),
                    area.title(),
                    checklist.name(),
                    trail,
                    item_line.to_string(),
                    guidance,
                    file,
                ));
                active_item = Some(items.len() - 1);
            }
        } else if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(index) = active_item {
                items[index].push_detail(line);
            }
        } else if !line.is_empty() {
            active_item = None;
        }
    }
}

#[instrument(level = "debug")]
fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.bytes().take_while(|byte| *byte == b'#').count();
    if (1..=6).contains(&level) && line.as_bytes().get(level) == Some(&b' ') {
        Some((level, line[level + 1..].trim()))
    } else {
        None
    }
}

#[instrument(level = "debug")]
fn italic_paragraph(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if let Some(value) = trimmed.strip_prefix('_').and_then(|s| s.strip_suffix('_')) {
        return Some(value);
    }
    if trimmed.starts_with("**") {
        return None;
    }
    trimmed.strip_prefix('*').and_then(|s| s.strip_suffix('*'))
}

#[instrument(level = "debug")]
fn file_hint(line: &str) -> Option<String> {
    let mut segments = line.split('`');
    while segments.next().is_some() {
        let Some(quoted) = segments.next() else {
            break;
        };
        let path = strip_location(quoted);
        if looks_like_source_path(path) {
            return Some(path.to_string());
        }
    }
    None
}

#[instrument(level = "debug")]
fn strip_location(quoted: &str) -> &str {
    let Some((path, line)) = quoted.rsplit_once(':') else {
        return quoted;
    };
    if !line.is_empty() && line.bytes().all(|byte| byte.is_ascii_digit()) {
        path
    } else {
        quoted
    }
}

#[instrument(level = "debug", skip(path))]
fn looks_like_source_path(path: &str) -> bool {
    let candidate = Path::new(path);
    candidate.is_absolute()
        || candidate
            .extension()
            .is_some_and(|extension| matches!(extension.to_str(), Some("rs" | "toml" | "md")))
}
