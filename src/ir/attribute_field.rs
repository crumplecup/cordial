//! Read one JSON object field without materializing the other encoded values.

use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, Visitor};
use serde_json::Value;

pub(super) fn read(bytes: &[u8], field: &str) -> Result<Option<Value>, rmp_serde::decode::Error> {
    Field(field).deserialize(&mut rmp_serde::Deserializer::from_read_ref(bytes))
}

struct Field<'a>(&'a str);

impl<'de> DeserializeSeed<'de> for Field<'_> {
    type Value = Option<Value>;

    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for Field<'_> {
    type Value = Option<Value>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON object")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut value = None;
        while let Some(key) = map.next_key::<String>()? {
            if key == self.0 {
                value = Some(map.next_value()?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(value)
    }
}
