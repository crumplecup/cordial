//! Kind variant payload findings for internal error architecture.

use std::collections::BTreeSet;

use crate::error::CordialResult;
use crate::etiquettes::internal_error_chain::type_graph::is_foreign_type_label;
use crate::etiquettes::internal_error_chain::types::{
    InternalErrorComplianceFinding, InternalErrorComplianceId,
};

use tracing::instrument;

use super::super::catalog::{Catalog, EnumInfo, VariantInfo};
use super::{FindingSite, SiteClassifier};

impl Catalog {
    #[instrument(level = "debug", skip(self, findings))]
    pub(super) fn emit_kind_variant_findings(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
    ) -> CordialResult<()> {
        let parent_idents: BTreeSet<String> = self
            .root_parents()
            .into_iter()
            .map(|item| item.ident().clone())
            .collect();
        for item in self.enums().values() {
            let lint = self.is_error_kind(item)
                || (self.impls_error(item.ident()) && Self::is_error_enum_name(item.ident()));
            if !lint {
                continue;
            }
            for variant in item.variants() {
                self.lint_variant_payloads(findings, item, variant, &parent_idents)?;
            }
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self, findings, item, variant, parent_idents))]
    fn lint_variant_payloads(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
        item: &EnumInfo,
        variant: &VariantInfo,
        parent_idents: &BTreeSet<String>,
    ) -> CordialResult<()> {
        let context = format!("{}::{}", item.type_path(), variant.name());
        if variant.payloads().len() != 1 {
            findings.push(self.kind_variant_finding(
                context,
                item,
                variant,
                format!(
                    "{} — Kind variant must be a 1-tuple native source",
                    variant.snippet()
                ),
                SiteClassifier::default(),
            )?);
            return Ok(());
        }

        let payload = &variant.payloads()[0];
        let payload_ident = Self::last_ident(payload).to_string();
        if self.emit_invalid_payload_shape(
            findings,
            item,
            variant,
            &context,
            payload,
            &payload_ident,
        )? {
            return Ok(());
        }
        if self.emit_invalid_payload_identity(
            findings,
            item,
            variant,
            &context,
            &payload_ident,
            parent_idents,
        )? {
            return Ok(());
        }
        self.emit_unregistered_payload_finding(findings, item, variant, context, &payload_ident)
    }

    #[instrument(
        level = "debug",
        skip(self, findings, item, variant, context, payload, payload_ident)
    )]
    fn emit_invalid_payload_shape(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
        item: &EnumInfo,
        variant: &VariantInfo,
        context: &str,
        payload: &str,
        payload_ident: &str,
    ) -> CordialResult<bool> {
        if is_foreign_type_label(payload) {
            findings.push(self.kind_variant_finding(
                context.to_string(),
                item,
                variant,
                format!(
                    "{} — wrap the foreign error in a native source, not `{payload}`",
                    variant.snippet()
                ),
                SiteClassifier::foreign(Some(payload.to_string())),
            )?);
            return Ok(true);
        }
        if payload_ident == "String" {
            findings.push(self.kind_variant_finding(
                context.to_string(),
                item,
                variant,
                format!("{} — String is not a native source", variant.snippet()),
                SiteClassifier::default(),
            )?);
            return Ok(true);
        }
        Ok(false)
    }

    #[instrument(
        level = "debug",
        skip(self, findings, item, variant, context, payload_ident, parent_idents)
    )]
    fn emit_invalid_payload_identity(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
        item: &EnumInfo,
        variant: &VariantInfo,
        context: &str,
        payload_ident: &str,
        parent_idents: &BTreeSet<String>,
    ) -> CordialResult<bool> {
        if Self::is_kind_name(payload_ident) || parent_idents.contains(payload_ident) {
            findings.push(self.kind_variant_finding(
                context.to_string(),
                item,
                variant,
                format!(
                    "{} — variant must hold a native source, not `{payload_ident}`",
                    variant.snippet()
                ),
                SiteClassifier::default(),
            )?);
            return Ok(true);
        }
        Ok(false)
    }

    #[instrument(level = "debug", skip(self, findings, item, variant, context))]
    fn emit_unregistered_payload_finding(
        &self,
        findings: &mut Vec<InternalErrorComplianceFinding>,
        item: &EnumInfo,
        variant: &VariantInfo,
        context: String,
        payload_ident: &str,
    ) -> CordialResult<()> {
        if !self.structs().contains_key(payload_ident) {
            findings.push(self.kind_variant_finding(
                context,
                item,
                variant,
                format!(
                    "{} — wrap `{payload_ident}` in a native source",
                    variant.snippet()
                ),
                SiteClassifier::default(),
            )?);
        } else if !self.impls_error(payload_ident) {
            findings.push(self.kind_variant_finding(
                context,
                item,
                variant,
                format!(
                    "{} — `{payload_ident}` must implement Error",
                    variant.snippet()
                ),
                SiteClassifier::default(),
            )?);
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self, item, variant, snippet, classifier))]
    fn kind_variant_finding(
        &self,
        context: String,
        item: &EnumInfo,
        variant: &VariantInfo,
        snippet: String,
        classifier: SiteClassifier,
    ) -> CordialResult<InternalErrorComplianceFinding> {
        self.finding(
            InternalErrorComplianceId::ArchKindVariant001,
            FindingSite::new(context, item.file().clone(), variant.line(), snippet),
            classifier,
        )
    }
}
