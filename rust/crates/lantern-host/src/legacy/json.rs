//! Private, ordered JSON inspection. Numeric lexemes never pass through Value/f64
//! storage. Duplicate keys are rejected at every level, including opaque payloads.

use crate::{BoundaryError, ErrorKind};
use serde::{
    Deserialize, Deserializer,
    de::{Error, MapAccess, Visitor},
};
use serde_json::value::RawValue;
use std::{collections::BTreeSet, fmt};

#[derive(Clone)]
pub(super) enum Node {
    Object(Vec<(String, Node)>),
    Array(Vec<Node>),
    Scalar(Box<RawValue>),
}

struct Object(Vec<(String, Box<RawValue>)>);

impl<'de> Deserialize<'de> for Object {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor;
        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = Object;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a unique-key object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Object, M::Error> {
                let mut seen = BTreeSet::new();
                let mut fields = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
                    if !seen.insert(key.clone()) {
                        return Err(M::Error::custom("duplicate key"));
                    }
                    fields.push((key, value));
                }
                Ok(Object(fields))
            }
        }
        deserializer.deserialize_map(ObjectVisitor)
    }
}

fn json_error(path: &str, error: serde_json::Error) -> BoundaryError {
    BoundaryError::at(
        path,
        ErrorKind::Json {
            line: error.line(),
            column: error.column(),
        },
    )
}

impl Node {
    pub fn parse(bytes: &[u8], path: &str) -> Result<Self, BoundaryError> {
        let raw = serde_json::from_slice(bytes).map_err(|e| json_error(path, e))?;
        Self::from_raw(raw, path, 0)
    }

    fn from_raw(raw: Box<RawValue>, path: &str, depth: usize) -> Result<Self, BoundaryError> {
        if depth > 128 {
            return Err(BoundaryError::invalid(path, "JSON nesting limit exceeded"));
        }
        match raw.get().as_bytes()[0] {
            b'{' => {
                let Object(fields) =
                    serde_json::from_str(raw.get()).map_err(|e| json_error(path, e))?;
                fields
                    .into_iter()
                    .enumerate()
                    .map(|(index, (key, raw))| {
                        Self::from_raw(raw, &format!("{path}.fields[{index}]"), depth + 1)
                            .map(|node| (key, node))
                    })
                    .collect::<Result<_, _>>()
                    .map(Self::Object)
            }
            b'[' => {
                let elements: Vec<Box<RawValue>> =
                    serde_json::from_str(raw.get()).map_err(|e| json_error(path, e))?;
                elements
                    .into_iter()
                    .enumerate()
                    .map(|(index, raw)| Self::from_raw(raw, &format!("{path}[{index}]"), depth + 1))
                    .collect::<Result<_, _>>()
                    .map(Self::Array)
            }
            _ => {
                let text = raw.get();
                if matches!(text.as_bytes()[0], b'-' | b'0'..=b'9')
                    && text.contains(['.', 'e', 'E'])
                    && !text.parse::<f64>().is_ok_and(f64::is_finite)
                {
                    return Err(BoundaryError::invalid(path, "nonfinite numeric value"));
                }
                Ok(Self::Scalar(raw))
            }
        }
    }

    pub fn object(&self, path: &str) -> Result<&[(String, Node)], BoundaryError> {
        match self {
            Self::Object(fields) => Ok(fields),
            _ => Err(BoundaryError::invalid(path, "expected object")),
        }
    }
    pub fn array(&self, path: &str) -> Result<&[Node], BoundaryError> {
        match self {
            Self::Array(elements) => Ok(elements),
            _ => Err(BoundaryError::invalid(path, "expected array")),
        }
    }
    pub fn optional(&self, key: &str, path: &str) -> Result<Option<&Node>, BoundaryError> {
        Ok(self
            .object(path)?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| value))
    }
    pub fn required(&self, key: &str, path: &str) -> Result<&Node, BoundaryError> {
        self.optional(key, path)?
            .ok_or_else(|| BoundaryError::invalid(format!("{path}.{key}"), "missing field"))
    }
    pub fn string(&self, path: &str) -> Result<String, BoundaryError> {
        match self {
            Self::Scalar(raw) => serde_json::from_str(raw.get())
                .map_err(|_| BoundaryError::invalid(path, "expected string")),
            _ => Err(BoundaryError::invalid(path, "expected string")),
        }
    }
    pub fn unsigned(&self, path: &str) -> Result<u64, BoundaryError> {
        match self {
            Self::Scalar(raw) => serde_json::from_str(raw.get())
                .map_err(|_| BoundaryError::invalid(path, "expected unsigned integer")),
            _ => Err(BoundaryError::invalid(path, "expected unsigned integer")),
        }
    }
    pub fn tile(&self, path: &str) -> Result<i32, BoundaryError> {
        match self {
            Self::Scalar(raw) => serde_json::from_str(raw.get())
                .map_err(|_| BoundaryError::invalid(path, "expected i32 tile")),
            _ => Err(BoundaryError::invalid(path, "expected i32 tile")),
        }
    }
    pub fn boolean(&self, path: &str) -> Result<bool, BoundaryError> {
        match self {
            Self::Scalar(raw) => serde_json::from_str(raw.get())
                .map_err(|_| BoundaryError::invalid(path, "expected boolean")),
            _ => Err(BoundaryError::invalid(path, "expected boolean")),
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Scalar(raw) if raw.get() == "null")
    }
    pub fn nonnegative_number(&self, path: &str) -> Result<f64, BoundaryError> {
        let Self::Scalar(raw) = self else {
            return Err(BoundaryError::invalid(path, "expected number"));
        };
        let text = raw.get();
        // Underflow must not turn a negative persisted timer into accepted -0.0.
        let negative = text.starts_with('-')
            && text
                .split(['e', 'E'])
                .next()
                .unwrap()
                .bytes()
                .any(|b| matches!(b, b'1'..=b'9'));
        text.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.0 && !negative)
            .ok_or_else(|| BoundaryError::invalid(path, "expected finite nonnegative number"))
    }

    pub fn seed_namespace(&self, path: &str) -> Result<String, BoundaryError> {
        let Self::Scalar(raw) = self else {
            return Err(BoundaryError::invalid(path, "unsupported chunk seed type"));
        };
        let text = raw.get();
        let seed = if text.starts_with('"') {
            self.string(path)?
        } else if text
            .strip_prefix('-')
            .unwrap_or(text)
            .bytes()
            .all(|b| b.is_ascii_digit())
        {
            if text == "-0" {
                "0".into()
            } else {
                text.into()
            }
        } else {
            return Err(BoundaryError::invalid(path, "unsupported chunk seed type"));
        };
        if seed.is_empty() || seed == "." || seed == ".." || seed.contains(['/', '\\', '\0', ':']) {
            return Err(BoundaryError::invalid(path, "unsafe chunk seed namespace"));
        }
        Ok(seed)
    }
}
