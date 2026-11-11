//! Typed initializer shapes collected from the shared tree and planned later.

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::super::Edit;

type Range = (usize, usize);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum StorageForm {
    Bare,
    Method,
    StringFrom,
    EmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum ValueSource {
    CallSiteFile,
    StaticString { local_binding: bool },
    EmptyFile,
}

/// One expression and the provenance needed to rewrite either storage form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new)]
pub(super) struct InitializerExpressionFact {
    whole: Range,
    value: Range,
    form: StorageForm,
    source: ValueSource,
    field_name: String,
    shorthand: bool,
}

impl InitializerExpressionFact {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn end(&self) -> usize {
        self.whole.1
    }

    #[instrument(level = "trace", skip(self, source, strategy))]
    pub(super) fn edit(&self, source: &str, strategy: ErrorFieldStrategy) -> Option<Edit> {
        match strategy {
            ErrorFieldStrategy::Borrowed if self.form == StorageForm::Bare => return None,
            ErrorFieldStrategy::Owned if self.form != StorageForm::Bare => return None,
            _ => {}
        }
        let value = source.get(self.value.0..self.value.1)?;
        let replacement = match (strategy, &self.source) {
            (ErrorFieldStrategy::Borrowed, ValueSource::EmptyFile) => "\"\"".to_string(),
            (ErrorFieldStrategy::Owned, ValueSource::EmptyFile) => "String::new()".to_string(),
            (ErrorFieldStrategy::Borrowed, _) => value.to_string(),
            (ErrorFieldStrategy::Owned, _) => match self.source {
                ValueSource::StaticString {
                    local_binding: true,
                } => format!("String::from({value})"),
                _ => format!("{value}.to_string()"),
            },
        };
        Some(Edit {
            start: self.whole.0,
            end: self.whole.1,
            replacement: if self.shorthand {
                format!("{}: {replacement}", self.field_name)
            } else {
                replacement
            },
        })
    }
}
