//! Conservative crate module and import path identities.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::loader::SharedParsedFile;

/// A cfg can remove a module without changing its privacy boundary when present.
/// Conditional identities do not establish an unconditional type-resolution path.
#[derive(Clone, serde::Serialize, serde::Deserialize, derive_getters::Getters)]
pub(in crate::etiquettes::internal_error_chain::apply) struct ModuleIdentity {
    path: Vec<String>,
    #[getter(copy)]
    unconditional: bool,
}

#[instrument(level = "debug", skip(files, source), err(level = "warn"))]
pub(in crate::etiquettes::internal_error_chain::apply) fn module_identities(
    files: &[SharedParsedFile],
    source: &crate::ir::SourceErrorApply,
) -> crate::CordialResult<HashMap<PathBuf, ModuleIdentity>> {
    let mut by_module: HashMap<Vec<String>, Vec<&SharedParsedFile>> = HashMap::new();
    for file in files {
        if let Some(module) = module_path(file.path()) {
            by_module.entry(module).or_default().push(file);
        }
    }
    let mut identities = HashMap::new();
    let mut pending = vec![(Vec::new(), true)];
    while let Some((module, unconditional)) = pending.pop() {
        let Some([file]) = by_module.get(&module).map(Vec::as_slice) else {
            continue;
        };
        identities.insert(
            file.path().to_path_buf(),
            ModuleIdentity {
                path: module.clone(),
                unconditional,
            },
        );
        for item in source.namespace_for(file)?.modules() {
            let mut child = module.clone();
            child.push(item.name().clone());
            if by_module.get(&child).is_some_and(|files| files.len() == 1) {
                pending.push((child, unconditional && item.unconditional()));
            }
        }
    }
    Ok(identities)
}

#[instrument(level = "debug", skip(path))]
fn module_path(path: &Path) -> Option<Vec<String>> {
    let src = path
        .ancestors()
        .find(|part| part.file_name().is_some_and(|name| name == "src"))?;
    let relative = path.strip_prefix(src).ok()?;
    let mut parts: Vec<String> = relative
        .iter()
        .map(|part| part.to_str().map(str::to_string))
        .collect::<Option<_>>()?;
    let file = parts.pop()?;
    let stem = file.strip_suffix(".rs")?;
    if !matches!(stem, "lib" | "main" | "mod") {
        parts.push(stem.to_string());
    }
    Some(parts)
}

#[instrument(level = "debug")]
pub(in crate::etiquettes::internal_error_chain::apply) fn absolute_path(
    module: &[String],
    written: &[String],
) -> Option<Vec<String>> {
    let (mut base, mut rest) = if written.first().is_some_and(|name| name == "crate") {
        (Vec::new(), &written[1..])
    } else if written.first().is_some_and(|name| name == "self") {
        (module.to_vec(), &written[1..])
    } else {
        (module.to_vec(), written)
    };
    while rest.first().is_some_and(|name| name == "super") {
        base.pop()?;
        rest = &rest[1..];
    }
    if rest.is_empty() {
        return None;
    }
    base.extend(rest.iter().cloned());
    Some(base)
}
