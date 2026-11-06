//! Select candidate-specific sites using only projected contracts and IR joins.
use super::super::{
    ConversionCandidate, CrateTypeIndex, ErrorConversionSite, ErrorConversionSiteKind,
};
use super::{ConversionFileFacts, Event, Kind, PathFact};
use crate::etiquettes::internal_error_chain::apply::initializers::FieldKind;
use crate::loader::SourceSnapshot;

impl PathFact {
    #[tracing::instrument(level = "trace", skip(self))]
    fn associated(&self, name: &str, owner: Option<&str>) -> bool {
        self.names.len() > 1
            && self
                .names
                .first()
                .is_some_and(|first| first == name || (first == "Self" && owner == Some(name)))
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn direct(&self, name: &str, owner: Option<&str>) -> bool {
        self.bare && (self.names == [name] || (self.names == ["Self"] && owner == Some(name)))
    }
}

impl ConversionFileFacts {
    #[tracing::instrument(level = "debug", skip(self, file, candidates, relevant, declarations))]
    pub(in crate::etiquettes::internal_error_chain::apply::inventory) fn select(
        &self,
        file: &impl SourceSnapshot,
        candidates: &[ConversionCandidate],
        relevant: &[usize],
        declarations: &CrateTypeIndex,
    ) -> Vec<Vec<ErrorConversionSite>> {
        let mut sites = vec![Vec::new(); candidates.len()];
        for event in &self.events {
            if event.callee_guards.iter().any(|guard| {
                relevant.iter().any(|&index| {
                    guard
                        .path
                        .associated(&candidates[index].type_name, guard.owner.as_deref())
                })
            }) {
                continue;
            }
            for &index in relevant {
                if let Some(site) = event.select(file, &candidates[index], declarations) {
                    sites[index].push(site);
                }
            }
        }
        sites
    }
}

impl Event {
    #[tracing::instrument(level = "trace", skip(self, file, candidate, declarations))]
    fn select(
        &self,
        file: &impl SourceSnapshot,
        candidate: &ConversionCandidate,
        declarations: &CrateTypeIndex,
    ) -> Option<ErrorConversionSite> {
        use ErrorConversionSiteKind as Site;
        let name = &candidate.type_name;
        let field = &candidate.field_name;
        let owner = self.owner.as_deref();
        let mut reference = None;
        let mut from_conversion = None;
        let mut source_method = None;
        let mut try_conversion = None;
        let mut edge = None;
        let (kind, resolved) = match &self.kind {
            Kind::Record { path, rest }
                if path.direct(name, owner) || path.names.last() == Some(name) =>
            {
                reference = Some(path.names.clone());
                (Site::Constructor, path.direct(name, owner) && !rest)
            }
            Kind::Caller(path) | Kind::Associated(path) if path.associated(name, owner) => {
                let mut parts = path.names.clone();
                parts.pop();
                reference = Some(parts);
                if matches!(self.kind, Kind::Caller(_)) {
                    (Site::Caller, !path.qualified && path.names.len() == 2)
                } else {
                    (Site::Alias, false)
                }
            }
            Kind::Field(member) if member == field => {
                if candidate.field.getter_storage.is_some()
                    && declarations.unrelated_getter_field(
                        &candidate.declaration,
                        name,
                        file.path(),
                        self.start,
                    )
                {
                    return None;
                }
                (Site::FieldUse, false)
            }
            Kind::Pattern { path, fields }
                if fields.contains(field)
                    && (path.direct(name, owner) || path.names.last() == Some(name)) =>
            {
                reference = Some(path.names.clone());
                (Site::FieldPattern, false)
            }
            Kind::Try { returns, evidence }
                if returns.as_ref().is_none_or(|names| {
                    names.contains(name) || (owner == Some(name) && names.contains("Self"))
                }) =>
            {
                try_conversion = evidence.clone();
                (Site::TryConversion, false)
            }
            Kind::Type(path) if path.names.last() == Some(name) => {
                reference = Some(path.names.clone());
                (Site::TypeUse, !path.qualified && path.names.len() == 1)
            }
            Kind::Alias {
                names,
                stable_error,
            } if names.contains(name) => (
                Site::Alias,
                candidate.field.kind == FieldKind::CallSiteFile
                    && stable_error.as_ref() == Some(name),
            ),
            Kind::Import { names, glob } if *glob || names.contains(name) => (Site::Alias, false),
            Kind::Macro {
                constructs,
                fields,
                unhandled,
            } if constructs.contains(name)
                || constructs.contains("Self")
                || fields.contains(field) =>
            {
                let constructs = constructs.contains(name) || constructs.contains("Self");
                if !constructs
                    && candidate.field.getter_storage.is_some()
                    && declarations.unrelated_getter_macro(
                        &candidate.declaration,
                        name,
                        file.path(),
                        self.start,
                        self.end,
                    )
                {
                    return None;
                }
                (Site::Macro, !constructs && !unhandled.contains(field))
            }
            Kind::From { mentions, evidence } if owner == Some(name) || mentions.contains(name) => {
                from_conversion = Some(evidence.clone());
                (Site::FromConversion, false)
            }
            Kind::Source(evidence) if owner == Some(name) => {
                source_method = Some(evidence.clone());
                (Site::SourceMethod, false)
            }
            Kind::Attribute {
                name: declaration,
                stable_file,
            } if declaration == name => {
                if *stable_file && candidate.field.kind == FieldKind::CallSiteFile {
                    return None;
                }
                (Site::GeneratedConstructor, false)
            }
            Kind::Edge {
                name: declaration,
                evidence,
            } if declaration == name => {
                edge = evidence.clone();
                (Site::ErrorEdge, edge.is_some())
            }
            _ => return None,
        };
        Some(ErrorConversionSite {
            path: file.path().to_path_buf(),
            line: self.start.0,
            column: self.start.1,
            kind,
            edge,
            from_conversion,
            source_method,
            try_conversion,
            reference_path: reference.filter(|_| self.depth == 0),
            target: None,
            field_accessible: true,
            resolved: resolved && file.path() == candidate.declaration && self.depth == 0,
        })
    }
}
