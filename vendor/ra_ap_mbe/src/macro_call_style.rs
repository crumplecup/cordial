//! Types representing the three basic "styles" of macro calls in Rust source:
//! - Function-like macros ("bang macros"), e.g. `foo!(...)`
//! - Attribute macros, e.g. `#[foo]`
//! - Derive macros, e.g. `#[derive(Foo)]`

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroCallStyle {
    FnLike,
    Attr,
    Derive,
}

bitflags::bitflags! {
    /// A set of `MacroCallStyle` values, allowing macros to indicate that
    /// they support more than one style.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct MacroCallStyles: u8 {
        const FN_LIKE = (1 << 0);
        const ATTR = (1 << 1);
        const DERIVE = (1 << 2);
    }
}

impl serde::Serialize for MacroCallStyles {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.bits())
    }
}

impl<'de> serde::Deserialize<'de> for MacroCallStyles {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <u8 as serde::Deserialize>::deserialize(deserializer).map(Self::from_bits_retain)
    }
}

impl From<MacroCallStyle> for MacroCallStyles {
    fn from(kind: MacroCallStyle) -> Self {
        match kind {
            MacroCallStyle::FnLike => Self::FN_LIKE,
            MacroCallStyle::Attr => Self::ATTR,
            MacroCallStyle::Derive => Self::DERIVE,
        }
    }
}
