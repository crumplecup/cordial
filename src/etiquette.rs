//! An **etiquette** is a named bundle of analysis hooks: loaders, enrichers,
//! probes, assessors, and reporters.
//!
//! Built-in etiquettes live under `src/etiquettes/`. Register one on a
//! [`crate::Session`] with [`crate::Session::register`], or run the CLI
//! (`cordial quality`, `cordial coverage`). `cordial explain` prints
//! [`Etiquette::explain`] for every bundle compiled into the binary.

mod explain;
mod handle;
mod hooks;
mod order;
mod order_table;
mod quality;
mod static_table;
mod traits;

pub use explain::{
    EtiquetteExplain, EtiquetteRuleExplain, lookup_etiquette, render_explain_list,
    render_explain_page,
};
pub use handle::IntoEtiquette;
pub use hooks::EtiquetteHooks;
pub(crate) use order::sort_quality_etiquettes;
pub use order::{
    After, BUILT_IN_ORDER, Before, BeforeMirror, DERIVE_RULE_IDS, LintConstraint, LintOrder,
    OrderExplain,
};
pub use quality::{QualityAreaSpec, QualityEtiquette, QualityReportArea};
pub(crate) use quality::{count_open_category, count_open_rule, finding_field, open_findings};
pub use static_table::{StaticEtiquette, StaticQualityEtiquette};
pub use traits::Etiquette;
