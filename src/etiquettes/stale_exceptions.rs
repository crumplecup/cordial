//! Exception rows that no longer suppress anything.
//!
//! **What.** Finds exception rows whose etiquette ran but produced no finding
//! the row matches.
//!
//! **Why.** Exceptions are keyed to a file, line, rule, or context. When code
//! moves or the finding is fixed, the row silently goes dead and the next
//! real finding at that spot is no longer covered by an intentional reason.
//! Dead rows must be re-keyed or removed, not left to rot.
//!
//! **Flags.** Every row as `EXCEPTION-STALE-001`. It is raised by the
//! assessment pass, which alone sees both the exception sets and the findings
//! of the run; this etiquette contributes only the reports.
//!
//! **Outputs.** `{store}/findings/stale-exceptions.checklist.md` and
//! `stale-exceptions-summary.md`.
//!
//! **Config.** Always on. Fix a row with `cordial exceptions edit` or
//! `cordial exceptions remove`. Register [`STALE_EXCEPTIONS_ETIQUETTE`].

mod reporter;
mod types;

pub use reporter::{StaleExceptionChecklistReporter, StaleExceptionSummaryReporter};
pub use types::{STALE_EXCEPTION_RULE_ID, StaleExceptionFinding};

use std::sync::LazyLock;

use crate::etiquette::{
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, QualityAreaSpec, StaticEtiquette,
    StaticQualityEtiquette, count_open_category,
};
use crate::objects::Finding;

use tracing::instrument;

static CHECKLIST: StaleExceptionChecklistReporter = StaleExceptionChecklistReporter;
static SUMMARY: StaleExceptionSummaryReporter = StaleExceptionSummaryReporter;

static REPORTERS: &[&'static dyn crate::Reporter] = &[&CHECKLIST, &SUMMARY];

/// Built-in stale-exceptions etiquette bundle.
pub static STALE_EXCEPTIONS_ETIQUETTE: LazyLock<StaticQualityEtiquette> = LazyLock::new(|| {
    StaticQualityEtiquette::new(
        StaticEtiquette::new(
            "stale_exceptions",
            "Stale exceptions",
            EtiquetteHooks::new(&[], &[], &[], &[], None, REPORTERS),
            false,
            EtiquetteExplain::new(
                "Does every exception row still suppress a finding?",
                "An exception keyed to a file, line, rule, or context goes dead when the code moves or the finding is fixed. A dead row hides nothing today and misleads tomorrow.",
                "Flags each exception row whose etiquette ran and produced no finding the row matches.",
                "None; fix the row with `cordial exceptions edit` or `cordial exceptions remove`.",
                vec![EtiquetteRuleExplain::new(
                    STALE_EXCEPTION_RULE_ID,
                    "An exception row that matches no finding",
                )],
            ),
        ),
        Some(QualityAreaSpec::new(
            "Stale exceptions",
            "stale-exceptions.checklist.md",
            "stale-exceptions-summary.md",
            quality_area_compute,
        )),
    )
});

#[instrument(level = "debug", skip(findings))]
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let stale = count_open_category(findings, "stale_exceptions");
    (stale, format!("stale exception rows **{stale}**"))
}
