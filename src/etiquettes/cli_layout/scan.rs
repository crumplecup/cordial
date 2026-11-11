//! CLI layout policy consumes only loader-projected evidence.
use super::{
    hunt::nested_clap_types,
    types::{CliLayoutId, CliLayoutRecord},
};
use crate::CordialResult;
use crate::ir::{CliLayoutFacts, EntryItem, LayoutCatalog, TypeRec};
use std::{collections::BTreeSet, path::PathBuf};
use tracing::instrument;
/// Apply the layout rules to the loader-projected CLI catalog.
#[instrument(level = "debug", skip(facts), err(level = "warn"))]
pub fn select_records(
    facts: &CliLayoutFacts,
    crate_name: &str,
) -> CordialResult<Vec<CliLayoutRecord>> {
    let catalog = facts.catalog();
    let mut findings = Vec::new();
    let clap_idents: BTreeSet<String> = catalog
        .types()
        .values()
        .filter(|item| item.parser() || item.subcommand())
        .map(|item| item.ident().clone())
        .collect();
    let has_clap = !clap_idents.is_empty();

    for item in catalog.types().values() {
        if item.in_library() {
            if item.parser() || item.subcommand() {
                lint_act(crate_name, item, catalog, &clap_idents, &mut findings)?;
            }
            continue;
        }
        if item.parser() || item.subcommand() || item.error() {
            findings.push(finding(
                crate_name,
                CliLayoutId::Island001,
                item.type_path().clone(),
                item.file().clone(),
                item.line(),
                format!(
                    "{} — CLI and error types belong in the library, not a binary island",
                    item.snippet()
                ),
            )?);
        }
    }

    if has_clap {
        for func in catalog.free_fns() {
            if !func.in_library() {
                continue;
            }
            for ident in func.input_idents() {
                if clap_idents.contains(ident) {
                    findings.push(finding(
                        crate_name,
                        CliLayoutId::Act001,
                        format!("{}::{ident}", catalog.crate_name()),
                        func.file().clone(),
                        func.line(),
                        format!(
                            "fn {} — dispatch `{ident}` with `{ident}::act`, not a free function",
                            func.name()
                        ),
                    )?);
                    break;
                }
            }
        }
        for entry in facts.entries() {
            lint_entry(crate_name, entry, &mut findings)?;
        }
    }

    Ok(findings)
}

#[instrument(
    level = "debug",
    skip(item, catalog, clap_idents, findings),
    err(level = "warn")
)]
fn lint_act(
    crate_name: &str,
    item: &TypeRec,
    catalog: &LayoutCatalog,
    clap_idents: &BTreeSet<String>,
    findings: &mut Vec<CliLayoutRecord>,
) -> CordialResult<()> {
    let Some(act) = catalog.acts().get(item.ident()) else {
        findings.push(finding(
            crate_name,
            CliLayoutId::Act001,
            item.type_path().clone(),
            item.file().clone(),
            item.line(),
            format!(
                "{} — write `fn act(self, …) -> Result<_, _>` on this clap type",
                item.snippet()
            ),
        )?);
        return Ok(());
    };
    let nested = nested_clap_types(item, clap_idents);
    let missing: Vec<String> = nested
        .into_iter()
        .filter(|name| !act.called_on().contains(name))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let names = missing.join(", ");
    findings.push(finding(
        crate_name,
        CliLayoutId::Act001,
        item.type_path().clone(),
        act.file().clone(),
        act.line(),
        format!(
            "{}::act must call `act` on nested clap type(s) `{names}`",
            item.ident()
        ),
    )?);
    Ok(())
}

#[instrument(level = "debug", skip(rule_id, file), err(level = "warn"))]
fn finding(
    crate_name: &str,
    rule_id: CliLayoutId,
    context: String,
    file: PathBuf,
    line: u32,
    snippet: String,
) -> CordialResult<CliLayoutRecord> {
    CliLayoutRecord::builder()
        .crate_name(crate_name.to_string())
        .rule_id(rule_id)
        .context(context)
        .file(file)
        .line(line)
        .snippet(snippet)
        .build()
}

#[instrument(level = "debug", skip(entry, findings), err(level = "warn"))]
fn lint_entry(
    crate_name: &str,
    entry: &EntryItem,
    findings: &mut Vec<CliLayoutRecord>,
) -> CordialResult<()> {
    use crate::ir::EntryKind;
    let name = entry.name();
    let mut push = |context: String, snippet: String| -> CordialResult<()> {
        findings.push(finding(
            crate_name,
            CliLayoutId::Main001,
            context,
            entry.file().clone(),
            entry.line(),
            snippet,
        )?);
        Ok(())
    };
    let context = format!("{crate_name}::{name}");
    match entry.kind() {
        EntryKind::Main {
            has_match,
            has_parse,
            has_act,
        } => {
            if has_match {
                push(
                    context.clone(),
                    "match in `main` — dispatch with `Cli::act`, not in `main`".into(),
                )?;
            }
            if !has_parse || !has_act {
                push(
                    context,
                    "`main` must call `parse` and `act` (then miette)".into(),
                )?;
            }
        }
        EntryKind::Function => push(
            context,
            format!("fn {name} — `main` should parse, call `act`, and convert to miette"),
        )?,
        EntryKind::Module => push(
            context,
            format!("mod {name} — dispatch and CLI types belong in the library"),
        )?,
        EntryKind::Struct => push(context, format!("struct {name} — keep `main` thin"))?,
        EntryKind::Enum => push(context, format!("enum {name} — keep `main` thin"))?,
        EntryKind::Impl => push(
            crate_name.to_string(),
            "impl in `main` — dispatch belongs on library types".into(),
        )?,
    }
    Ok(())
}
