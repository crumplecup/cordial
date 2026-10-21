//! Review Kind variants that route many distinct literal failures into one source.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use crate::error::CordialResult;
use crate::etiquettes::internal_error_chain::types::{
    InternalErrorComplianceFinding, InternalErrorComplianceId,
};
use tracing::instrument;

use super::super::catalog::{Catalog, FanoutTarget};
use super::{FindingSite, SiteClassifier};

impl Catalog {
    #[instrument(level = "debug", skip(self, findings))]
    pub(super) fn emit_kind_fanout_findings(
        &self,
        max_call_sites: usize,
        findings: &mut Vec<InternalErrorComplianceFinding>,
    ) -> CordialResult<()> {
        let mut sites: BTreeMap<FanoutTarget, BTreeMap<(PathBuf, u32), String>> = BTreeMap::new();
        let wrappers: HashMap<_, _> = self
            .fanout_wrappers()
            .iter()
            .map(|wrapper| {
                (
                    (
                        wrapper.file().as_path(),
                        wrapper.module().as_slice(),
                        wrapper.name().as_str(),
                    ),
                    wrapper.target(),
                )
            })
            .collect();
        for call in self.fanout_calls() {
            let target = if let Some(target) = call.target() {
                Some(target)
            } else {
                wrappers
                    .get(&(
                        call.file().as_path(),
                        call.module().as_slice(),
                        call.callee().as_str(),
                    ))
                    .copied()
            };
            let Some(target) = target.and_then(|target| self.resolved_fanout_target(target)) else {
                continue;
            };
            sites
                .entry(target)
                .or_default()
                .insert((call.file().clone(), call.line()), call.literal().clone());
        }

        for (target, calls) in sites {
            let literals: BTreeSet<_> = calls.values().collect();
            if calls.len() <= max_call_sites || literals.len() <= max_call_sites {
                continue;
            }
            let context = match (target.kind(), target.variant()) {
                (Some(kind), Some(variant)) => format!("{kind}::{variant}"),
                _ => format!("{}::new", target.source()),
            };
            let evidence = calls
                .iter()
                .map(|((file, line), literal)| format!("{}:{line} {literal:?}", file.display()))
                .collect::<Vec<_>>()
                .join("; ");
            let (file, line) = self.fanout_anchor(&target, &calls);
            let guidance = if target.kind().is_some() {
                "review whether these failures need distinct variants"
            } else {
                "review whether these callers collapse distinct failures into one Kind variant"
            };
            findings.push(self.finding(
                InternalErrorComplianceId::ArchKindFanout001,
                FindingSite::new(
                    context,
                    file,
                    line,
                    format!(
                        "{} literal call sites feed {}::new; {guidance} — {evidence}",
                        calls.len(),
                        target.source()
                    ),
                ),
                SiteClassifier::constructor(None, format!("{}::new", target.source())),
            )?);
        }
        Ok(())
    }

    #[instrument(level = "trace", skip(self, target))]
    fn resolved_fanout_target(&self, target: &FanoutTarget) -> Option<FanoutTarget> {
        if let (Some(kind), Some(variant)) = (target.kind(), target.variant()) {
            if let Some(item) = self.enums().get(kind) {
                let matches = item.variants().iter().any(|entry| {
                    entry.name() == variant
                        && entry.payloads().len() == 1
                        && Self::last_ident(&entry.payloads()[0]) == target.source()
                });
                return matches.then(|| target.clone());
            }
            // The Kind may be defined in another workspace member. The call
            // expression still supplies both the variant and source identity.
            return Some(target.clone());
        }
        let mut matches = self
            .enums()
            .values()
            .filter(|item| self.is_error_kind(item))
            .flat_map(|item| {
                item.variants().iter().filter_map(move |variant| {
                    (variant.payloads().len() == 1
                        && Self::last_ident(&variant.payloads()[0]) == target.source())
                    .then(|| {
                        FanoutTarget::new(
                            Some(item.ident().clone()),
                            Some(variant.name().clone()),
                            target.source().clone(),
                        )
                    })
                })
            });
        match (matches.next(), matches.next()) {
            (Some(first), None) => Some(first),
            (None, None) => Some(target.clone()),
            _ => None,
        }
    }

    #[instrument(level = "trace", skip(self, target, calls))]
    fn fanout_anchor(
        &self,
        target: &FanoutTarget,
        calls: &BTreeMap<(PathBuf, u32), String>,
    ) -> (PathBuf, u32) {
        if let Some(kind) = target.kind()
            && let Some(item) = self.enums().get(kind)
            && let Some(variant) = item
                .variants()
                .iter()
                .find(|variant| Some(variant.name()) == target.variant().as_ref())
        {
            return (item.file().clone(), variant.line());
        }
        if let Some(wrapper) = self
            .fanout_wrappers()
            .iter()
            .find(|wrapper| self.resolved_fanout_target(wrapper.target()).as_ref() == Some(target))
        {
            return (wrapper.file().clone(), wrapper.line());
        }
        calls.keys().next().cloned().unwrap_or_default()
    }
}
