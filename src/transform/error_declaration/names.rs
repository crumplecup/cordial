//! Deterministic names and rendering for the first owned source contract.

/// Generated names are output obligations, not proof of an existing declaration.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct GeneratedErrorNames {
    parent: String,
    kind: String,
    source: String,
}

impl GeneratedErrorNames {
    #[tracing::instrument(level = "trace", skip(name))]
    pub(super) fn for_site(name: &str, site_index: usize) -> Option<Self> {
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
        Some(Self {
            kind: format!("{parent}Kind"),
            source: format!("{parent}Source"),
            parent,
        })
    }

    #[tracing::instrument(level = "trace", skip(self, identifiers))]
    pub(super) fn collision<'a>(&'a self, identifiers: &[String]) -> Option<&'a str> {
        [&self.parent, &self.kind, &self.source]
            .into_iter()
            .find_map(|name| {
                identifiers
                    .iter()
                    .any(|identifier| identifier == name)
                    .then_some(name.as_str())
            })
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn render(&self) -> String {
        include_str!("owned_io.rs.txt")
            .replace("$PARENT", &self.parent)
            .replace("$KIND", &self.kind)
            .replace("$SOURCE", &self.source)
    }
}
