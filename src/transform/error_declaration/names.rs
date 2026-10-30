//! Deterministic names for supported owned native-source alternatives.

use super::native::NativeError;

/// One generated wrapper paired with its concrete native source identity.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct GeneratedErrorSource {
    variant: String,
    wrapper: String,
    native_type: String,
}

/// Generated names are output obligations, not proof of an existing declaration.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct GeneratedErrorNames {
    parent: String,
    kind: String,
    /// Primary wrapper retained for single-source consumers; sources lists all.
    source: String,
    sources: Vec<GeneratedErrorSource>,
}

impl GeneratedErrorNames {
    #[tracing::instrument(level = "trace", skip(name, natives))]
    pub(super) fn for_site(name: &str, site_index: usize, natives: &[NativeError]) -> Option<Self> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return None;
        }
        let stem = name
            .split('_')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let (first, rest) = part.split_at(1);
                format!("{}{rest}", first.to_ascii_uppercase())
            })
            .collect::<String>();
        let parent = format!("Cordial{stem}Site{site_index}Error");
        let source = format!("{parent}Source");
        let sources = natives
            .iter()
            .enumerate()
            .map(|(index, native)| GeneratedErrorSource {
                variant: native.variant().to_owned(),
                wrapper: if index == 0 {
                    source.clone()
                } else {
                    format!("{source}{}", native.variant())
                },
                native_type: native.identity().to_owned(),
            })
            .collect();
        Some(Self {
            kind: format!("{parent}Kind"),
            source,
            sources,
            parent,
        })
    }

    #[tracing::instrument(level = "trace", skip(self, identifiers))]
    pub(super) fn collision<'a>(&'a self, identifiers: &[String]) -> Option<&'a str> {
        [&self.parent, &self.kind]
            .into_iter()
            .chain(self.sources.iter().map(|source| &source.wrapper))
            .find_map(|name| {
                identifiers
                    .iter()
                    .any(|identifier| identifier == name)
                    .then_some(name.as_str())
            })
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn render(&self) -> String {
        let variants = self
            .sources
            .iter()
            .map(|source| format!("    {}({}),", source.variant, source.wrapper))
            .collect::<Vec<_>>()
            .join("\n");
        let display = self
            .sources
            .iter()
            .map(|source| {
                format!(
                    "            Self::{}(source) => ::core::fmt::Display::fmt(source, formatter),",
                    source.variant
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let sources = self
            .sources
            .iter()
            .map(|source| {
                format!(
                    "            $KIND::{}(source) => ::core::option::Option::Some(source),",
                    source.variant
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut output = include_str!("owned_parent.rs.txt")
            .replace("$VARIANTS", &variants)
            .replace("$DISPLAY", &display)
            .replace("$SOURCES", &sources)
            .replace("$PARENT", &self.parent)
            .replace("$KIND", &self.kind);
        for source in &self.sources {
            for template in [
                include_str!("owned_source.rs.txt"),
                include_str!("owned_source_conversion.rs.txt"),
            ] {
                output.push_str(
                    &template
                        .replace("$PARENT", &self.parent)
                        .replace("$KIND", &self.kind)
                        .replace("$SOURCE", &source.wrapper)
                        .replace("$NATIVE", &source.native_type)
                        .replace("$VARIANT", &source.variant),
                );
            }
        }
        output
    }
}
