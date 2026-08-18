//! Parse rustdoc `Stability { … }` markers from attribute debug text.

use nom::{
    IResult,
    branch::alt,
    bytes::complete::{tag, take_until},
    combinator::value,
    sequence::preceded,
};
use rustdoc_types::{Attribute, Item};

use tracing::instrument;
const STABILITY_LEVEL_PREFIX: &str = "Stability {stability: Stability {level: ";

/// Stability classification extracted from rustdoc JSON item attrs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StabilityLevel {
    /// `level: Stable { since: … }` — accountable in stable std scope.
    Stable,
    /// `level: Unstable { … }` — nightly / feature-gated.
    Unstable,
    /// No `Stability` marker on this item's attrs.
    #[default]
    Unknown,
}

impl StabilityLevel {
    #[instrument(level = "trace", skip(self), ret)]
    pub fn is_unstable(self) -> bool {
        matches!(self, Self::Unstable)
    }

    #[instrument(level = "trace", skip(self), ret)]
    pub fn is_stable(self) -> bool {
        matches!(self, Self::Stable)
    }
}

/// Parse a [`StabilityLevel`] from one `Attribute::Other` debug string.
#[instrument(level = "debug")]
pub fn parse_stability_attr_text(text: &str) -> StabilityLevel {
    if let Some(level) = parse_stability_level(text).ok().map(|(_, level)| level) {
        return level;
    }
    // Older HIR debug forms that omit the nested `Stability { … }` wrapper.
    if text.contains("level: Unstable") {
        return StabilityLevel::Unstable;
    }
    if text.contains("level: Stable") {
        return StabilityLevel::Stable;
    }
    StabilityLevel::Unknown
}

/// Combined stability from all attrs on an item (Unstable wins over Stable).
#[instrument(level = "debug")]
pub fn stability_from_attrs(attrs: &[Attribute]) -> StabilityLevel {
    let mut saw_stable = false;
    for attr in attrs {
        let rustdoc_types::Attribute::Other(text) = attr else {
            continue;
        };
        match parse_stability_attr_text(text) {
            StabilityLevel::Unstable => return StabilityLevel::Unstable,
            StabilityLevel::Stable => saw_stable = true,
            StabilityLevel::Unknown => {}
        }
    }
    if saw_stable {
        StabilityLevel::Stable
    } else {
        StabilityLevel::Unknown
    }
}

/// Whether an item's own attrs mark it unstable.
#[instrument(level = "debug")]
pub fn item_attrs_are_unstable(item: &Item) -> bool {
    stability_from_attrs(&item.attrs).is_unstable()
}

/// True when rustdoc JSON embeds parseable stability markers (sanity check for sysroot cache).
#[instrument(level = "debug")]
pub fn rustdoc_json_has_stability_markers(content: &str) -> bool {
    content.contains(STABILITY_LEVEL_PREFIX)
}

fn parse_stability_level(input: &str) -> IResult<&str, StabilityLevel> {
    let (rest, _) = take_until(STABILITY_LEVEL_PREFIX)(input)?;
    let (rest, _) = tag(STABILITY_LEVEL_PREFIX)(rest)?;
    preceded(
        nom::character::complete::multispace0,
        alt((
            value(StabilityLevel::Unstable, tag("Unstable")),
            value(StabilityLevel::Stable, tag("Stable")),
        )),
    )(rest)
}
