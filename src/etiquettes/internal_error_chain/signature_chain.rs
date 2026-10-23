//! Conservative source-chain check across locally resolved conversion signatures.
//!
//! A call is rejected only when its input and output types are known and the
//! output's verified `Error::source` shape cannot reach the input type.
//! Unresolved calls and opaque source implementations remain unknown.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::{Expr, Type};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Ty {
    path: String,
    args: Vec<Ty>,
}

#[derive(Clone)]
struct Function {
    scope: Vec<String>,
    params: Vec<Type>,
    output: Type,
    generics: BTreeSet<String>,
    self_ty: Option<Ty>,
}

#[derive(Clone)]
struct ErrorType {
    scope: Vec<String>,
    params: Vec<String>,
    sources: Vec<Type>,
    variant_sources: BTreeMap<String, Type>,
    error_impl: bool,
}

#[derive(Clone)]
enum ManualSource {
    Default,
    NamedField,
    Variants(Vec<String>),
    Opaque,
}

#[derive(Clone)]
struct Conversion {
    scope: Vec<String>,
    receiver: Expr,
    body: Expr,
    binding: String,
    params: BTreeMap<String, Type>,
    file: PathBuf,
    line: u32,
}

/// Result of resolving one conversion against the verified source graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureChainStatus {
    /// The output error type can retain the caught error as a source.
    Preserved,
    /// The output error type cannot retain the caught error as a source.
    Broken,
    /// The call, input type, or source relationship could not be resolved.
    Unknown,
}

/// Signature-level assessment of a conversion site.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct SignatureChainCheck {
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    status: SignatureChainStatus,
    input_type: Option<String>,
    output_type: Option<String>,
}

impl SignatureChainCheck {
    #[tracing::instrument(level = "debug", skip(self))]
    pub(crate) fn strip_file_prefix(&mut self, root: &Path) {
        if let Ok(rel) = self.file.strip_prefix(root) {
            self.file = rel.to_path_buf();
        }
    }
}

/// Signature and verified source relationships gathered from the shared syntax trees.
#[derive(Default)]
pub(crate) struct SignatureChainIndex {
    functions: BTreeMap<String, Vec<Function>>,
    errors: BTreeMap<String, Vec<ErrorType>>,
    manual_impls: BTreeMap<String, Vec<ManualSource>>,
    imports: BTreeMap<Vec<String>, BTreeMap<String, String>>,
    conversions: Vec<Conversion>,
}

mod collect;
mod resolve;
mod syntax;
