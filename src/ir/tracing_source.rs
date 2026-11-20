//! Loader-owned function evidence for tracing policy, independent of a live AST.

use serde::{Deserialize, Serialize};

mod native;
mod recordable;

use crate::etiquettes::tracing::{FnContext, FunctionKind, VisibilityLabel};

/// File-level tracing inventory stored on its source module.
pub const ATTR_SOURCE_TRACING_INVENTORY: &str = "source_tracing_inventory";

/// A declaration collected from the shared source tree. Paths are file-local;
/// the owning module supplies the file and module prefix during enrichment.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_getters::Getters,
    derive_builder::Builder,
)]
#[builder(build_fn(error = "crate::CordialError"))]
pub struct SourceTracingFunction {
    path: String,
    range: super::SourceByteRange,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    kind: FunctionKind,
    visibility: VisibilityLabel,
    #[getter(copy)]
    instrumented: bool,
    #[getter(copy)]
    prover_visible_instrument: bool,
    context: FnContext,
    /// Hotpath syntax evidence collected from the same canonical source tree.
    #[serde(default)]
    #[builder(default)]
    hotpath: Option<SourceHotpathFunction>,
}

/// Snapshot-bound inventory. Even an empty source file has this fact; absence
/// means collection was omitted, rather than that every function is compliant.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct SourceTracingInventory {
    #[getter(copy)]
    source_digest: [u8; 32],
    functions: Vec<SourceTracingFunction>,
}

/// Syntax evidence, not proof that a function executed or a cfg was enabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceHotpathFunction {
    /// An unconditional, directly spelled Hotpath measurement attribute.
    pub measured: bool,
    /// Conditional attributes need producer-feature validation.
    pub conditional: bool,
    /// Async bodies require poll-aware measurement.
    pub asynchronous: bool,
    /// Closures may escape the enclosing function or execute on another worker.
    pub closures: usize,
    /// Async blocks need their own poll-aware scope if they escape.
    pub async_blocks: usize,
}
