//! Resolve type text to a [`TypeKey`] through allowlisted rustdoc crates.

use std::collections::HashMap;

use rustdoc_types::{GenericArg, GenericArgs, GenericParamDefKind, Id, ItemEnum, Type, TypeAlias};
use tracing::instrument;

use super::{CrateIndex, Lookup, TypeKey, TypeText, normalize_type_text, parse_type_text};

/// Seam for turning type text into a canonical identity. Matching code
/// depends on this trait, not on rustdoc.
pub trait TypeResolver {
    /// Resolve `text` fully: head and every argument.
    fn resolve(&self, text: &str) -> Result<TypeKey, Unresolved>;

    /// Resolve only the head, ignoring arguments. For generic claims whose
    /// arguments are type parameters (`ExtGeneric<DateTime<Tz>>`).
    fn resolve_head(&self, text: &str) -> Result<TypeKey, Unresolved>;

    /// Whether `ty` has a direct impl of the trait named by `bound`.
    fn implements(&self, ty: &TypeKey, bound: &str) -> Implements;
}

/// Whether a type is known to satisfy a trait bound.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum Implements {
    /// A direct `impl Trait for Type` exists.
    #[display("yes")]
    Yes,
    /// The type was found and has no direct impl of the trait. A blanket
    /// impl would not be seen, so this is "not found", not "does not".
    #[display("no direct impl found")]
    NoDirectImpl,
    /// The check could not be made, with the reason.
    #[display("unknown: {_0}")]
    Unknown(String),
}

/// Why a type could not be given an identity. Never a silent match or miss.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum Unresolved {
    /// No allowlisted crate has an item by that name.
    #[display("unknown name `{_0}`")]
    UnknownName(String),
    /// Several distinct types answer to a bare name.
    #[display("`{name}` is ambiguous: {}", candidates.join(", "))]
    Ambiguous {
        /// The name as written.
        name: String,
        /// The distinct canonical types it could mean.
        candidates: Vec<String>,
    },
    /// Alias expansion chained past `alias_depth`.
    #[display("alias depth exceeded at `{_0}`")]
    AliasDepthExceeded(String),
    /// An expanded type grew past `max_type_nodes`.
    #[display("type too large at `{_0}`")]
    TypeTooLarge(String),
    /// The text is not a well-formed type (unbalanced brackets, empty).
    #[display("malformed type `{_0}`")]
    Malformed(String),
    /// The name resolves to something that is not a type.
    #[display("`{_0}` is not a type")]
    NotAType(String),
    /// A form the resolver does not expand (e.g. a reference inside an alias).
    #[display("unsupported type form in `{_0}`")]
    Unsupported(String),
}

/// Bounds on resolution work; see `[amenable_ext]` in `cordial.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct ResolveCaps {
    /// Max chained type-alias expansions per name.
    #[getter(copy)]
    alias_depth: usize,
    /// Max nodes in one expanded [`TypeKey`].
    #[getter(copy)]
    max_type_nodes: usize,
}

impl Default for ResolveCaps {
    #[instrument(level = "debug")]
    fn default() -> Self {
        Self::new(8, 64)
    }
}

/// [`TypeResolver`] over a set of allowlisted rustdoc crates.
///
/// A path whose first segment names an allowlisted crate is looked up in it.
/// A path under any other crate is *opaque*: kept as spelled, never
/// expanded. A bare name is searched in every allowlisted crate's root
/// namespace; one distinct answer resolves, several are `Ambiguous`.
#[derive(Debug, derive_new::new)]
pub struct RustdocTypeResolver {
    crates: Vec<CrateIndex>,
    caps: ResolveCaps,
}

impl TypeResolver for RustdocTypeResolver {
    #[instrument(level = "trace", skip(self))]
    fn resolve(&self, text: &str) -> Result<TypeKey, Unresolved> {
        self.resolve_normalized(text, false)
    }

    #[instrument(level = "trace", skip(self))]
    fn resolve_head(&self, text: &str) -> Result<TypeKey, Unresolved> {
        self.resolve_normalized(text, true)
    }

    #[instrument(level = "trace", skip(self, ty))]
    fn implements(&self, ty: &TypeKey, bound: &str) -> Implements {
        self.check_implements(ty, bound)
    }
}

impl RustdocTypeResolver {
    #[instrument(level = "debug", skip(self, ty))]
    fn check_implements(&self, ty: &TypeKey, bound: &str) -> Implements {
        let bound_key = match self.resolve_head(bound) {
            Ok(key) => key,
            Err(reason) => return Implements::Unknown(format!("bound `{bound}`: {reason}")),
        };
        let owner = ty.head().split("::").next().unwrap_or_default();
        let Some(index) = self.crate_named(owner) else {
            return Implements::Unknown(format!("`{ty}` is outside the allowlisted crates"));
        };
        let Some(id) = index.item_defined_at(ty.head()) else {
            return Implements::Unknown(format!("`{ty}` was not found in `{owner}`"));
        };
        for trait_id in index.implemented_trait_ids(&id) {
            if let Ok(found) = self.key_for_item(index, &trait_id, Vec::new(), 0)
                && found.head() == bound_key.head()
            {
                return Implements::Yes;
            }
        }
        Implements::NoDirectImpl
    }

    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    fn resolve_normalized(&self, text: &str, head_only: bool) -> Result<TypeKey, Unresolved> {
        let parsed = parse_type_text(&normalize_type_text(text))
            .ok_or_else(|| Unresolved::Malformed(text.to_string()))?;
        self.resolve_text(&parsed, head_only)
    }

    #[instrument(level = "debug", skip(self, text), err(level = "warn"))]
    fn resolve_text(&self, text: &TypeText, head_only: bool) -> Result<TypeKey, Unresolved> {
        let args = if head_only {
            Vec::new()
        } else {
            text.args()
                .iter()
                .map(|arg| self.resolve_text(arg, false))
                .collect::<Result<Vec<_>, _>>()?
        };
        let key = self.resolve_path(text.path(), args)?;
        self.check_size(key)
    }

    #[instrument(level = "trace", skip(self))]
    fn crate_named(&self, name: &str) -> Option<&CrateIndex> {
        self.crates.iter().find(|index| index.name() == name)
    }

    #[instrument(level = "debug", skip(self, key), err(level = "warn"))]
    fn check_size(&self, key: TypeKey) -> Result<TypeKey, Unresolved> {
        if key.node_count() > self.caps.max_type_nodes() {
            return Err(Unresolved::TypeTooLarge(key.head().clone()));
        }
        Ok(key)
    }

    #[instrument(level = "debug", skip(self, path, args), err(level = "warn"))]
    fn resolve_path(&self, path: &str, args: Vec<TypeKey>) -> Result<TypeKey, Unresolved> {
        if !path
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
        {
            return Ok(TypeKey::new(path.to_string(), args));
        }
        let segments: Vec<&str> = path.split("::").collect();
        let mut hits: Vec<(&CrateIndex, Id)> = Vec::new();
        if segments.len() > 1 {
            let Some(index) = self.crate_named(segments[0]) else {
                return Ok(TypeKey::new(path.to_string(), args));
            };
            collect_hits(&mut hits, index, index.lookup(&segments[1..]));
        } else {
            for index in &self.crates {
                collect_hits(&mut hits, index, index.lookup(&segments));
            }
        }
        if hits.is_empty() {
            return Err(Unresolved::UnknownName(path.to_string()));
        }
        let mut keys: Vec<TypeKey> = Vec::new();
        let mut first_error = None;
        for (index, id) in hits {
            match self.key_for_item(index, &id, args.clone(), 0) {
                Ok(key) if !keys.contains(&key) => keys.push(key),
                Ok(_) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        match (keys.len(), first_error) {
            (1, _) => Ok(keys.remove(0)),
            (0, Some(error)) => Err(error),
            _ => Err(Unresolved::Ambiguous {
                name: path.to_string(),
                candidates: keys.iter().map(ToString::to_string).collect(),
            }),
        }
    }

    /// Identity of the item `id` seen from crate `index`, applied to `args`.
    /// Type aliases expand; everything else keeps its canonical head.
    #[instrument(level = "debug", skip(self, index, id, args), err(level = "warn"))]
    fn key_for_item(
        &self,
        index: &CrateIndex,
        id: &Id,
        args: Vec<TypeKey>,
        depth: usize,
    ) -> Result<TypeKey, Unresolved> {
        let Some(item) = index.item(id) else {
            return self.key_for_external(index, id, args, depth);
        };
        let key = match &item.inner {
            ItemEnum::TypeAlias(alias) => {
                let name = item.name.clone().unwrap_or_default();
                self.expand_alias(index, alias, args, depth + 1, &name)?
            }
            ItemEnum::Struct(_)
            | ItemEnum::Enum(_)
            | ItemEnum::Union(_)
            | ItemEnum::Trait(_)
            | ItemEnum::TraitAlias(_)
            | ItemEnum::Primitive(_) => TypeKey::new(canonical_head(index, id)?, args),
            _ => {
                return Err(Unresolved::NotAType(
                    item.name.clone().unwrap_or_else(|| format!("{id:?}")),
                ));
            }
        };
        self.check_size(key)
    }

    /// An id this crate's index does not hold: re-resolve it in the
    /// allowlisted crate that owns it, else keep it opaque.
    #[instrument(level = "debug", skip(self, index, id, args), err(level = "warn"))]
    fn key_for_external(
        &self,
        index: &CrateIndex,
        id: &Id,
        args: Vec<TypeKey>,
        depth: usize,
    ) -> Result<TypeKey, Unresolved> {
        let Some((crate_id, segments)) = index.path_of(id) else {
            return Err(Unresolved::UnknownName(format!("{id:?}")));
        };
        if crate_id != 0
            && let Some((owner, rest)) = segments.split_first()
            && let Some(other) = self.crate_named(owner)
            && other.name() != index.name()
        {
            let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
            if let Lookup::Found(found) = other.lookup(&rest) {
                return self.key_for_item(other, &found, args, depth);
            }
        }
        Ok(TypeKey::new(segments.join("::"), args))
    }

    #[instrument(level = "debug", skip(self, index, alias, args), err(level = "warn"))]
    fn expand_alias(
        &self,
        index: &CrateIndex,
        alias: &TypeAlias,
        args: Vec<TypeKey>,
        depth: usize,
        name: &str,
    ) -> Result<TypeKey, Unresolved> {
        if depth > self.caps.alias_depth() {
            return Err(Unresolved::AliasDepthExceeded(name.to_string()));
        }
        let params = alias
            .generics
            .params
            .iter()
            .filter(|param| matches!(param.kind, GenericParamDefKind::Type { .. }))
            .map(|param| param.name.clone());
        let substitutions: HashMap<String, TypeKey> = params.zip(args).collect();
        self.type_to_key(index, &alias.type_, &substitutions, depth, name)
    }

    #[instrument(
        level = "debug",
        skip(self, index, ty, substitutions),
        err(level = "warn")
    )]
    fn type_to_key(
        &self,
        index: &CrateIndex,
        ty: &Type,
        substitutions: &HashMap<String, TypeKey>,
        depth: usize,
        alias_name: &str,
    ) -> Result<TypeKey, Unresolved> {
        match ty {
            Type::Generic(param) => substitutions
                .get(param)
                .cloned()
                .ok_or_else(|| Unresolved::UnknownName(param.clone())),
            Type::ResolvedPath(path) => {
                let mut args = Vec::new();
                if let Some(generic_args) = &path.args
                    && let GenericArgs::AngleBracketed { args: written, .. } = &**generic_args
                {
                    for arg in written {
                        if let GenericArg::Type(inner) = arg {
                            args.push(self.type_to_key(
                                index,
                                inner,
                                substitutions,
                                depth,
                                alias_name,
                            )?);
                        }
                    }
                }
                self.key_for_item(index, &path.id, args, depth)
            }
            _ => Err(Unresolved::Unsupported(alias_name.to_string())),
        }
    }
}

/// Add every id a lookup found to `hits`.
#[instrument(level = "debug", skip(hits, index, found))]
fn collect_hits<'a>(hits: &mut Vec<(&'a CrateIndex, Id)>, index: &'a CrateIndex, found: Lookup) {
    match found {
        Lookup::Found(id) => hits.push((index, id)),
        Lookup::Ambiguous(ids) => hits.extend(ids.into_iter().map(|id| (index, id))),
        Lookup::Missing => {}
    }
}

/// Canonical head for a local item: its defining path from rustdoc.
#[instrument(level = "debug", skip(index, id), err(level = "warn"))]
fn canonical_head(index: &CrateIndex, id: &Id) -> Result<String, Unresolved> {
    index
        .path_of(id)
        .map(|(_, segments)| segments.join("::"))
        .ok_or_else(|| Unresolved::UnknownName(format!("{id:?}")))
}
