//! Closed built-in constraint table. Validated in `const`.

mod constraints;
mod explains;
mod ids;

pub use ids::DERIVE_RULE_IDS;
#[cfg(feature = "quality")]
pub(crate) use ids::ERROR_HANDLING_RULE_IDS;

use crate::etiquette::order::LintOrder;
use constraints::CONSTRAINTS;
use ids::KNOWN_IDS;

/// Built-in lint order. A cycle or dangling id fails compilation.
pub static BUILT_IN_ORDER: std::sync::LazyLock<LintOrder> =
    std::sync::LazyLock::new(|| match LintOrder::try_from(KNOWN_IDS, CONSTRAINTS) {
        Ok(order) => order,
        Err(_) => LintOrder::empty(),
    });
