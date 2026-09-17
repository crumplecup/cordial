//! Explain-page metadata and rendering.

use std::fmt::Write;

use tracing::instrument;

use super::traits::Etiquette;

/// One rule id this etiquette can emit, so `cordial explain RULE-ID`
/// resolves to the etiquette page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EtiquetteRuleExplain {
    id: &'static str,
    summary: &'static str,
}

impl EtiquetteRuleExplain {
    /// Bind a stable rule id to its one-line note.
    pub const fn new(id: &'static str, summary: &'static str) -> Self {
        Self { id, summary }
    }

    /// Stable rule identifier (`DOC-WARNING-001`).
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// One-line decision note for that rule.
    pub const fn summary(&self) -> &'static str {
        self.summary
    }
}

/// Why this etiquette exists and how to opt out.
///
/// Mandatory on [`crate::etiquette::StaticEtiquette`] (no [`Default`]): a
/// constructor call missing an argument is a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EtiquetteExplain {
    summary: &'static str,
    why: &'static str,
    logic: &'static str,
    opt_out: &'static str,
    rules: &'static [EtiquetteRuleExplain],
}

impl EtiquetteExplain {
    /// Bind the explain page for a static etiquette table.
    ///
    /// This text is user-facing through `cordial explain`, so keep it specific
    /// enough to explain the standard without requiring source-code context.
    pub const fn new(
        summary: &'static str,
        why: &'static str,
        logic: &'static str,
        opt_out: &'static str,
        rules: &'static [EtiquetteRuleExplain],
    ) -> Self {
        Self {
            summary,
            why,
            logic,
            opt_out,
            rules,
        }
    }

    /// One line for `cordial explain` with no argument.
    pub const fn summary(&self) -> &'static str {
        self.summary
    }

    /// Why the check exists.
    pub const fn why(&self) -> &'static str {
        self.why
    }

    /// What is flagged, what is ignored, how the scan works.
    pub const fn logic(&self) -> &'static str {
        self.logic
    }

    /// `[panics] enabled = false` in cordial.toml; not rustc lint levels.
    pub const fn opt_out(&self) -> &'static str {
        self.opt_out
    }

    /// Rule ids that alias this page.
    pub const fn rules(&self) -> &'static [EtiquetteRuleExplain] {
        self.rules
    }
}

/// First etiquette whose id or rule id equals `query`.
#[instrument(level = "debug", skip(etiquettes))]
pub fn lookup_etiquette<'a>(
    etiquettes: &[&'a dyn Etiquette],
    query: &str,
) -> Option<&'a dyn Etiquette> {
    etiquettes
        .iter()
        .copied()
        .find(|etiquette| etiquette.id() == query)
        .or_else(|| {
            etiquettes.iter().copied().find(|etiquette| {
                etiquette
                    .explain()
                    .rules()
                    .iter()
                    .any(|rule| rule.id() == query)
            })
        })
}

/// One line per etiquette: id, then the one-line summary, sorted by id.
#[instrument(level = "debug", skip(etiquettes))]
pub fn render_explain_list(etiquettes: &[&dyn Etiquette]) -> String {
    let mut rows: Vec<(&str, &str)> = etiquettes
        .iter()
        .map(|etiquette| (etiquette.id(), etiquette.explain().summary()))
        .collect();
    rows.sort_by(|left, right| left.0.cmp(right.0));
    let width = rows.iter().map(|(id, _)| id.len()).max().unwrap_or(0);
    let mut body = String::new();
    for (id, summary) in rows {
        let _ = writeln!(body, "{id:<width$}  {summary}");
    }
    body
}

/// Full explain page for one etiquette.
#[instrument(level = "debug", skip(etiquette))]
pub fn render_explain_page(etiquette: &dyn Etiquette) -> String {
    let explain = etiquette.explain();
    let mut body = format!(
        "# {} (`{}`)\n\n{}\n\n## Why\n\n{}\n\n## Logic\n\n{}\n\n## Opt out\n\n{}\n",
        etiquette.name(),
        etiquette.id(),
        explain.summary(),
        explain.why(),
        explain.logic(),
        explain.opt_out(),
    );
    if !explain.rules().is_empty() {
        body.push_str("\n## Rules\n\n");
        for rule in explain.rules() {
            let _ = writeln!(body, "- `{}` — {}", rule.id(), rule.summary());
        }
    }
    body
}
