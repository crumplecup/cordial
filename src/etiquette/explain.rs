//! Explain-page metadata and rendering.

use std::fmt::Write;

use tracing::instrument;

use super::traits::Etiquette;

/// One rule id this etiquette can emit, so `cordial explain RULE-ID`
/// resolves to the etiquette page.
///
/// Owned `String` fields, built through one plain constructor. Built-ins
/// pass `&'static str` literals (free `Into<String>` conversion); etiquettes
/// derived from `cordial.toml` at runtime pass owned strings. Either way the
/// text is fixed once built, which is what actually keeps exceptions and
/// reports matching across runs — that guarantee never came from `const`,
/// just from nothing mutating these fields after construction.
#[derive(Debug, Clone, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct EtiquetteRuleExplain {
    /// Stable rule identifier (`DOC-WARNING-001`).
    #[new(into)]
    id: String,
    /// One-line decision note for that rule.
    #[new(into)]
    summary: String,
}

/// Why this etiquette exists and how to opt out.
///
/// Mandatory on [`crate::etiquette::StaticEtiquette`] (no [`Default`]): a
/// constructor call missing an argument is a compile error.
///
/// Deliberately a plain constructor, not `derive_builder`, despite 5
/// fields — same reasoning as [`crate::etiquette::StaticEtiquette`]: every
/// field is always required with no real validation, and every call site
/// builds inside a `LazyLock::new(|| ...)` closure that can't propagate a
/// builder's `Result`, so the only alternative is `.expect()`/`.unwrap()`
/// panics in library code for a guarantee the type signature already
/// gives for free.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct EtiquetteExplain {
    /// One line for `cordial explain` with no argument.
    summary: String,
    /// Why the check exists.
    why: String,
    /// What is flagged, what is ignored, how the scan works.
    logic: String,
    /// `[panics] enabled = false` in cordial.toml; not rustc lint levels.
    opt_out: String,
    /// Rule ids that alias this page.
    rules: Vec<EtiquetteRuleExplain>,
}

impl EtiquetteExplain {
    /// Bind the explain page for an etiquette table.
    ///
    /// This text is user-facing through `cordial explain`, so keep it specific
    /// enough to explain the standard without requiring source-code context.
    #[instrument(level = "debug", skip(summary, why, logic, opt_out, rules))]
    pub fn new(
        summary: impl Into<String>,
        why: impl Into<String>,
        logic: impl Into<String>,
        opt_out: impl Into<String>,
        rules: Vec<EtiquetteRuleExplain>,
    ) -> Self {
        Self {
            summary: summary.into(),
            why: why.into(),
            logic: logic.into(),
            opt_out: opt_out.into(),
            rules,
        }
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
                let explain = etiquette.explain();
                explain.rules().iter().any(|rule| rule.id() == query)
            })
        })
}

/// One line per etiquette: id, then the one-line summary, sorted by id.
#[instrument(level = "debug", skip(etiquettes))]
pub fn render_explain_list(etiquettes: &[&dyn Etiquette]) -> String {
    let mut rows: Vec<(&str, String)> = etiquettes
        .iter()
        .map(|etiquette| (etiquette.id(), etiquette.explain().summary().to_string()))
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
    append_resolution_order(&mut body, etiquette);
    body
}

#[instrument(level = "debug", skip(body, etiquette))]
fn append_resolution_order(body: &mut String, etiquette: &dyn Etiquette) {
    let explain = etiquette.explain();
    let rule_ids: Vec<&str> = explain
        .rules()
        .iter()
        .map(|rule| rule.id().as_str())
        .collect();
    let order = &super::order::BUILT_IN_ORDER;
    let after_rows = order.constraints_for(&rule_ids);
    let before_ids: Vec<&str> = rule_ids
        .iter()
        .filter(|id| order.before_mirror(id).is_some())
        .copied()
        .collect();
    if after_rows.is_empty() && before_ids.is_empty() {
        return;
    }

    body.push_str("\n## Resolution order\n\n");
    if let Some(row) = after_rows.first() {
        let explain = row.explain();
        let _ = writeln!(body, "{}\n", explain.title());
        let _ = writeln!(body, "{}\n", explain.body());
        let _ = writeln!(
            body,
            "Runs after: {}",
            row.after()
                .iter()
                .map(|id| format!("`{id}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if let Some(id) = before_ids.first()
        && let Some(mirror) = order.before_mirror(id)
    {
        if after_rows.is_empty() {
            let explain = mirror.explain();
            let _ = writeln!(body, "{}\n", explain.title());
            let _ = writeln!(body, "{}\n", explain.body());
        }
        let _ = writeln!(
            body,
            "Runs before: {}",
            mirror
                .before()
                .iter()
                .map(|id| format!("`{id}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}
