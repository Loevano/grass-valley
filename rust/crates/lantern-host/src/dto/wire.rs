use serde::{
    Deserialize, Deserializer, Serialize,
    de::{Error, MapAccess, Visitor},
};
use std::{collections::BTreeMap, fmt, marker::PhantomData};

/// Exact canonical decimal strings; never JavaScript numbers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Decimal(pub(crate) u64);

impl TryFrom<String> for Decimal {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty()
            || !value.bytes().all(|b| b.is_ascii_digit())
            || (value.len() > 1 && value.starts_with('0'))
        {
            return Err("expected canonical unsigned decimal string");
        }
        value
            .parse()
            .map(Self)
            .map_err(|_| "unsigned decimal overflow")
    }
}

impl From<Decimal> for String {
    fn from(value: Decimal) -> Self {
        value.0.to_string()
    }
}

/// Private field prevents NaN/Infinity from being serialized as JSON null.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(into = "f64")]
pub struct Number(f64);

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Inspect the JSON scalar before any lossy numeric conversion. DTO enums
        // use external tags so numeric values are never buffered through f64.
        let raw = Box::<serde_json::value::RawValue>::deserialize(deserializer)?;
        let text = raw.get();
        if !text.contains(['.', 'e', 'E']) {
            let magnitude = text.strip_prefix('-').unwrap_or(text).parse::<u64>();
            if !magnitude.is_ok_and(|value| value <= (1_u64 << 53)) {
                return Err(D::Error::custom(
                    "integer quantity exceeds exact binary64 range",
                ));
            }
        }
        let value = text
            .parse::<f64>()
            .map_err(|_| D::Error::custom("expected finite number"))?;
        Self::try_from(value).map_err(D::Error::custom)
    }
}

impl Number {
    pub(crate) fn get(self) -> f64 {
        self.0
    }
    pub(crate) fn core(value: f64) -> Self {
        Self::try_from(value).expect("finite core unit")
    }
}

impl TryFrom<f64> for Number {
    type Error = &'static str;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if !value.is_finite() {
            return Err("expected finite number");
        }
        Ok(Self(value))
    }
}

impl From<Number> for f64 {
    fn from(value: Number) -> Self {
        value.0
    }
}

/// JSON objects must not silently overwrite repeated goods/accounts/tokens.
#[derive(Serialize)]
#[serde(transparent)]
pub struct UniqueMap<V>(pub BTreeMap<String, V>);

impl<'de, V: Deserialize<'de>> Deserialize<'de> for UniqueMap<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor<V>(PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for UniqueVisitor<V> {
            type Value = UniqueMap<V>;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an object with unique keys")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, V>()? {
                    if values.insert(key, value).is_some() {
                        return Err(M::Error::custom("duplicate object key"));
                    }
                }
                Ok(UniqueMap(values))
            }
        }
        deserializer.deserialize_map(UniqueVisitor(PhantomData))
    }
}
