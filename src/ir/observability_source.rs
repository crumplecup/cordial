//! Loader-owned, configuration-independent evidence for tracing policies.

use crate::loader::{SourceLoadView, quality_scan_trees};
use crate::{CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod body;
mod compatibility;
mod native;

/// Crate-wide tracing evidence attached to the shared graph root.
pub const ATTR_SOURCE_OBSERVABILITY: &str = "source_observability";

/// Subscriber, executable-boundary and stdio inputs from canonical source trees.
/// Required fields distinguish a complete empty inventory from missing evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceObservability {
    crate_root: PathBuf,
    #[getter(copy)]
    has_lib: bool,
    #[getter(copy)]
    has_bin: bool,
    files: Vec<ObservabilityFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ObservabilityFileKind {
    Lib,
    Bin,
    Test,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct ObservabilityFile {
    snapshot: FileSnapshot,
    relative: PathBuf,
    #[getter(copy)]
    kind: ObservabilityFileKind,
    #[getter(copy)]
    syntax_valid: bool,
    functions: Vec<ObservabilityFunction>,
    prints: Vec<ObservabilityPrint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct ObservabilityFunction {
    name: String,
    modules: Vec<String>,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    test: bool,
    body: ObservabilityBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct ObservabilityPrint {
    context: String,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    rule: crate::etiquettes::tracing::PrintRuleId,
    #[getter(copy)]
    cargo_protocol: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct ObservabilityBody {
    #[getter(copy)]
    calls_install: bool,
    calls_try_init: bool,
    has_once: bool,
    has_try_from_default_env: bool,
    has_rust_log_literal: bool,
    has_fallback: bool,
    #[getter(copy)]
    is_fallible: bool,
    has_err_instrument: bool,
    has_error_emission: bool,
    called_names: Vec<String>,
    called_paths: Vec<String>,
}

impl SourceObservability {
    #[tracing::instrument(level = "debug", skip(source, file), err(level = "warn"))]
    pub(crate) fn from_fixture(
        source: &str,
        file: &Path,
        tree: &Path,
        root: &Path,
    ) -> CordialResult<Self> {
        let file = SourceLoadView::parse_fixture(file, source)?;
        Ok(Self {
            crate_root: root.to_path_buf(),
            has_lib: false,
            has_bin: false,
            files: vec![compatibility::collect(&file, tree, root)],
        })
    }

    #[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceObservability")]
    pub(crate) fn collect(source: &SourceLoadView, root: &Path) -> CordialResult<Self> {
        Self::collect_with(root, |path, tree| {
            match source.parsed_file(path, "source-observability") {
                Ok(file) => Ok(compatibility::collect(&file, tree, root)),
                Err(error)
                    if crate::loader::path_has_fixtures(path, root)
                        && matches!(error.kind(), crate::CordialErrorKind::SynParse(_)) =>
                {
                    let text = source
                        .input_text(path)?
                        .ok_or_else(|| crate::CordialError::not_found(path.to_path_buf()))?;
                    Ok(Self::invalid_file(path, root, &text))
                }
                Err(error) => Err(error),
            }
        })
    }

    #[tracing::instrument(level = "debug", skip(collect), err(level = "warn"))]
    pub(crate) fn collect_with(
        root: &Path,
        mut collect: impl FnMut(&Path, &Path) -> CordialResult<ObservabilityFile>,
    ) -> CordialResult<Self> {
        let mut files = Vec::new();
        for tree in quality_scan_trees(root) {
            for entry in walkdir::WalkDir::new(&tree).sort_by_file_name() {
                let entry = entry.map_err(anyhow::Error::new)?;
                if entry.file_type().is_file()
                    && entry.path().extension().is_some_and(|ext| ext == "rs")
                {
                    files.push(collect(entry.path(), &tree)?);
                }
            }
        }
        files.sort_by(|a, b| a.relative.cmp(&b.relative));
        Ok(Self {
            crate_root: root.to_path_buf(),
            has_lib: root.join("src/lib.rs").is_file(),
            has_bin: root.join("src/main.rs").is_file() || root.join("src/bin").is_dir(),
            files,
        })
    }
}

impl SourceObservability {
    #[tracing::instrument(level = "debug", skip(text, path))]
    pub(crate) fn invalid_file(path: &Path, root: &Path, text: &str) -> ObservabilityFile {
        use sha2::{Digest, Sha256};
        ObservabilityFile {
            snapshot: FileSnapshot::new(path.to_path_buf(), Sha256::digest(text.as_bytes()).into()),
            relative: path.strip_prefix(root).unwrap_or(path).to_path_buf(),
            kind: ObservabilityFileKind::for_path(path, root),
            syntax_valid: false,
            functions: Vec::new(),
            prints: Vec::new(),
        }
    }
}

impl ObservabilityFileKind {
    #[tracing::instrument(level = "debug", skip(path))]
    fn for_path(path: &Path, root: &Path) -> Self {
        let relative = path.strip_prefix(root).unwrap_or(path);
        if relative.starts_with("tests") {
            Self::Test
        } else if relative == Path::new("src/main.rs") || relative.starts_with("src/bin") {
            Self::Bin
        } else {
            Self::Lib
        }
    }
}

impl ObservabilityFunction {
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn context(&self, crate_name: &str) -> String {
        std::iter::once(crate_name)
            .chain(self.modules.iter().map(String::as_str))
            .chain(std::iter::once(self.name.as_str()))
            .collect::<Vec<_>>()
            .join("::")
    }
}

#[tracing::instrument(level = "debug")]
fn print_rule(name: &str) -> Option<crate::etiquettes::tracing::PrintRuleId> {
    use crate::etiquettes::tracing::PrintRuleId;
    match name {
        "println" => Some(PrintRuleId::Println),
        "eprintln" => Some(PrintRuleId::Eprintln),
        "print" => Some(PrintRuleId::Print),
        "eprint" => Some(PrintRuleId::Eprint),
        "dbg" => Some(PrintRuleId::Dbg),
        _ => None,
    }
}

#[tracing::instrument(level = "debug")]
fn cargo_protocol(tokens: &str) -> bool {
    let Some(start) = tokens.find('"') else {
        return false;
    };
    let rest = &tokens[start + 1..];
    rest.find('"')
        .is_some_and(|end| rest[..end].starts_with("cargo:"))
}

#[tracing::instrument(level = "trace", skip(syntax))]
fn has_err_instrument(syntax: &crate::ir::SourceInstrumentSyntax) -> bool {
    use crate::etiquettes::tracing::{InstrumentLevel, PresentInstrument};
    let present = PresentInstrument::from_syntax(syntax);
    !present.unresolved()
        && present
            .err_level()
            .is_some_and(|level| level >= InstrumentLevel::Warn)
}
