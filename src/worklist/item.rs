use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CordialError, CordialResult};
use tracing::instrument;
/// Stable handle for an item on a worklist.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(String);

impl ItemId {
    /// Text shown in CLI commands.
    #[instrument(level = "trace", skip(self))]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[instrument(level = "debug")]
    pub(super) fn from_string(value: String) -> Self {
        Self(value)
    }
}

/// An agent's claim about a checklist item, before the next gate run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    /// The item has not been acted on.
    Open,
    /// The agent claims to have fixed it.
    Done,
    /// The agent deferred it until the next list.
    Skipped,
}

/// One unchecked checklist entry, with its source context and current state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct WorklistItem {
    id: ItemId,
    scoring_key: String,
    area_priority: u8,
    area: String,
    checklist: String,
    heading_trail: Vec<String>,
    item_line: String,
    detail_lines: Vec<String>,
    guidance: Option<String>,
    file: Option<String>,
    state: ItemState,
    reopened: bool,
}

impl WorklistItem {
    #[instrument(level = "debug", skip(file))]
    pub(super) fn from_checklist(
        area_priority: u8,
        area: &str,
        checklist: &str,
        heading_trail: Vec<String>,
        item_line: String,
        guidance: Option<String>,
        file: Option<String>,
    ) -> Self {
        let scoring_key =
            serde_json::json!([area, heading_trail, without_line_number(&item_line)]).to_string();
        let digest = Sha256::digest(scoring_key.as_bytes());
        let id = ItemId(format!(
            "{:02x}{:02x}{:02x}",
            digest[0], digest[1], digest[2]
        ));
        Self {
            id,
            scoring_key,
            area_priority,
            area: area.to_string(),
            checklist: checklist.to_string(),
            heading_trail,
            item_line,
            detail_lines: Vec::new(),
            guidance,
            file,
            state: ItemState::Open,
            reopened: false,
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn push_detail(&mut self, line: &str) {
        self.detail_lines.push(line.to_string());
    }

    #[instrument(level = "trace", skip(self, state))]
    pub(super) fn replace_state(&mut self, state: ItemState) -> ItemState {
        std::mem::replace(&mut self.state, state)
    }
}

/// Remove location-only line numbers without changing other item text.
#[instrument(level = "debug")]
fn without_line_number(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b':' {
            let mut end = index + 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            let token_start = input[..index]
                .rfind(|character: char| character.is_whitespace() || character == '`')
                .map_or(0, |position| position + 1);
            let before_colon = &input[token_start..index];
            let is_location = before_colon.ends_with(".rs")
                || before_colon.ends_with(".toml")
                || before_colon.ends_with(".md");
            if end > index + 1 && is_location {
                output.push_str(&input[start..index]);
                start = end;
                index = end;
                continue;
            }
        }
        if input[index..].starts_with("(line ") {
            let mut end = index + "(line ".len();
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > index + "(line ".len() && bytes.get(end) == Some(&b')') {
                output.push_str(&input[start..index]);
                start = end + 1;
                index = start;
                continue;
            }
        }
        index += 1;
        while index < bytes.len() && !input.is_char_boundary(index) {
            index += 1;
        }
    }
    output.push_str(&input[start..]);
    output
}

/// Assign unique, stable display ids after all checklist items are known.
#[instrument(level = "debug", skip(items))]
pub(super) fn assign_ids(items: &mut [WorklistItem]) -> CordialResult<()> {
    let mut full_hashes = BTreeMap::new();
    for item in items.iter() {
        let digest = Sha256::digest(item.scoring_key().as_bytes());
        let hash: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        full_hashes.insert(item.scoring_key().clone(), hash);
    }
    let mut prefixes = BTreeMap::new();
    for (key, hash) in &full_hashes {
        let length = (6..=hash.len())
            .find(|length| {
                full_hashes.iter().all(|(other_key, other_hash)| {
                    other_key == key || hash[..*length] != other_hash[..*length]
                })
            })
            .ok_or_else(|| CordialError::invariant("distinct scoring keys share a full digest"))?;
        prefixes.insert(key, &hash[..length]);
    }
    let mut occurrences = HashMap::<String, usize>::new();
    for item in items {
        let key = item.scoring_key().clone();
        let occurrence = occurrences.entry(key.clone()).or_default();
        *occurrence += 1;
        let prefix = prefixes
            .get(&key)
            .ok_or_else(|| CordialError::invariant("missing worklist id prefix"))?;
        let id = if *occurrence == 1 {
            (*prefix).to_string()
        } else {
            format!("{prefix}-{occurrence}")
        };
        item.id = ItemId::from_string(id);
    }
    Ok(())
}
