//! Human-readable effective tier assignments for the current project.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::{
    CordialConfig, CordialResult, Etiquette, SessionBuilder, Tier, all_plugins, assigned_tier,
    builtin_tier, default_store_home, etiquettes_from_plugins, render_explain_page,
};
use tracing::instrument;

#[instrument(level = "debug", err(level = "warn"))]
pub(super) fn execute_tiers(project_root: &Path, store_home: Option<PathBuf>) -> CordialResult<()> {
    let session = SessionBuilder::new(project_root)
        .with_store_home(store_home.unwrap_or_else(default_store_home))
        .build();
    let config = crate::load_session_config(&session);
    let owned = etiquettes_from_plugins(&all_plugins(), &session);
    let etiquettes: Vec<&dyn Etiquette> = owned.iter().map(|item| item.as_ref()).collect();
    write!(io::stdout(), "{}", render_tiers(&etiquettes, &config))?;
    Ok(())
}

#[instrument(level = "debug", skip(etiquettes, config))]
fn render_tiers(etiquettes: &[&dyn Etiquette], config: &CordialConfig) -> String {
    let mut grouped: BTreeMap<String, Vec<&dyn Etiquette>> = BTreeMap::new();
    for name in [Tier::QUICK, Tier::BUILD, Tier::DEEP] {
        grouped.entry(name.to_string()).or_default();
    }
    for name in config.tiers().assign().values() {
        grouped.entry(name.clone()).or_default();
    }
    for etiquette in etiquettes {
        grouped
            .entry(assigned_tier(*etiquette, config).as_str().to_owned())
            .or_default()
            .push(*etiquette);
    }
    let mut body = format!("Default tiers: {}\n", config.tiers().default().join(", "));
    let mut order: Vec<String> = [Tier::QUICK, Tier::BUILD, Tier::DEEP]
        .into_iter()
        .map(str::to_string)
        .collect();
    let extra: Vec<String> = grouped
        .keys()
        .filter(|name| !order.contains(name))
        .cloned()
        .collect();
    order.extend(extra);
    for tier in order {
        let Some(rows) = grouped.get_mut(&tier) else {
            continue;
        };
        rows.sort_by(|left, right| left.id().cmp(right.id()));
        let _ = writeln!(body, "\n{tier}:");
        for etiquette in rows {
            let disabled = if config.etiquette_enabled(etiquette.id()) {
                ""
            } else {
                ", disabled"
            };
            let _ = writeln!(
                body,
                "  {} ({tier_source}{disabled})",
                etiquette.id(),
                tier_source = assignment_source(*etiquette, config),
            );
        }
    }
    body
}

#[instrument(level = "trace", skip(etiquette, config))]
fn assignment_source(etiquette: &dyn Etiquette, config: &CordialConfig) -> &'static str {
    if config.tiers().assign().contains_key(etiquette.id()) {
        "config"
    } else if builtin_tier(etiquette.id()).is_some() {
        "built-in"
    } else {
        "plugin"
    }
}

#[instrument(level = "debug", skip(etiquettes, config))]
pub(super) fn render_explain_list_with_tiers(
    etiquettes: &[&dyn Etiquette],
    config: &CordialConfig,
) -> String {
    let mut rows: Vec<_> = etiquettes.iter().copied().collect();
    rows.sort_by(|left, right| left.id().cmp(right.id()));
    let width = rows.iter().map(|item| item.id().len()).max().unwrap_or(0);
    let mut body = String::new();
    for etiquette in rows {
        let _ = writeln!(
            body,
            "{:<width$}  [{}]  {}",
            etiquette.id(),
            assigned_tier(etiquette, config).as_str(),
            etiquette.explain().summary(),
        );
    }
    body
}

#[instrument(level = "debug", skip(etiquette, config))]
pub(super) fn render_explain_page_with_tier(
    etiquette: &dyn Etiquette,
    config: &CordialConfig,
) -> String {
    let page = render_explain_page(etiquette);
    let tier = assigned_tier(etiquette, config);
    page.replacen(
        "\n\n",
        &format!(
            "\n\n**Tier:** `{}` ({})\n\n",
            tier.as_str(),
            assignment_source(etiquette, config)
        ),
        1,
    )
}
