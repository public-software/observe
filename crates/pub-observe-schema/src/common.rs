//! The messages every signal shares: `AnyValue`, `KeyValue` and the instrumentation scope.

use serde::{Deserialize, Serialize};

use crate::wire;

/// A value of any type: an attribute value, a log body, an element of an array or a map.
///
/// On the wire it is an object with exactly one key naming the kind (`stringValue`, `boolValue`,
/// `intValue` as a decimal string, `doubleValue`, `arrayValue`, `kvlistValue`, `bytesValue` as
/// base64); an object with no key or two is refused.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AnyValue {
    /// A string.
    #[serde(rename = "stringValue")]
    String(String),
    /// A boolean.
    #[serde(rename = "boolValue")]
    Bool(bool),
    /// A 64-bit signed integer, a decimal string on the wire.
    #[serde(rename = "intValue")]
    Int(#[serde(with = "wire::i64_str")] i64),
    /// A double.
    #[serde(rename = "doubleValue")]
    Double(f64),
    /// An array of values, possibly empty and possibly of mixed kinds.
    #[serde(rename = "arrayValue")]
    Array(#[serde(with = "values_list")] Vec<AnyValue>),
    /// A list of key-value pairs, a map whose keys must be unique.
    #[serde(rename = "kvlistValue")]
    KvList(#[serde(with = "values_list")] Vec<KeyValue>),
    /// Raw bytes, base64 on the wire.
    #[serde(rename = "bytesValue")]
    Bytes(#[serde(with = "wire::base64_bytes")] Vec<u8>),
}

impl From<&str> for AnyValue {
    fn from(value: &str) -> Self {
        AnyValue::String(value.to_owned())
    }
}

impl From<String> for AnyValue {
    fn from(value: String) -> Self {
        AnyValue::String(value)
    }
}

impl From<bool> for AnyValue {
    fn from(value: bool) -> Self {
        AnyValue::Bool(value)
    }
}

impl From<i64> for AnyValue {
    fn from(value: i64) -> Self {
        AnyValue::Int(value)
    }
}

impl From<f64> for AnyValue {
    fn from(value: f64) -> Self {
        AnyValue::Double(value)
    }
}

impl From<Vec<AnyValue>> for AnyValue {
    fn from(values: Vec<AnyValue>) -> Self {
        AnyValue::Array(values)
    }
}

impl From<Vec<KeyValue>> for AnyValue {
    fn from(values: Vec<KeyValue>) -> Self {
        AnyValue::KvList(values)
    }
}

impl From<Vec<u8>> for AnyValue {
    fn from(bytes: Vec<u8>) -> Self {
        AnyValue::Bytes(bytes)
    }
}

/// The `{"values": [...]}` wrapper of `arrayValue` and `kvlistValue`.
mod values_list {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize)]
    struct Out<'a, T> {
        #[serde(skip_serializing_if = "<[T]>::is_empty")]
        values: &'a [T],
    }

    #[derive(Deserialize)]
    #[serde(bound = "T: Deserialize<'de>")]
    struct In<T> {
        #[serde(default = "Vec::new")]
        values: Vec<T>,
    }

    pub fn serialize<S: Serializer, T: Serialize>(
        values: &[T],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Out { values }.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<Vec<T>, D::Error> {
        In::deserialize(deserializer).map(|list| list.values)
    }
}

/// A key and its value; the element of every attribute list and of a `kvlistValue`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyValue {
    /// The key; unique within its list.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key: String,
    /// The value; absent when the producer set none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<AnyValue>,
}

impl KeyValue {
    /// The pair `key` = `value`.
    pub fn new(key: impl Into<String>, value: impl Into<AnyValue>) -> Self {
        KeyValue {
            key: key.into(),
            value: Some(value.into()),
        }
    }
}

/// The instrumentation scope that produced a signal: a library, a module, a service.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentationScope {
    /// The name of the scope; empty means unknown.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The version of the scope; empty means unknown.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// Attributes describing the scope; keys unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<KeyValue>,
    /// How many attributes the producer dropped.
    #[serde(default, skip_serializing_if = "wire::is_default")]
    pub dropped_attributes_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(value: &AnyValue) -> String {
        serde_json::to_string(value).unwrap()
    }

    #[test]
    fn every_kind_has_its_key() {
        assert_eq!(wire(&AnyValue::from("s")), r#"{"stringValue":"s"}"#);
        assert_eq!(wire(&AnyValue::from(true)), r#"{"boolValue":true}"#);
        assert_eq!(wire(&AnyValue::from(-3i64)), r#"{"intValue":"-3"}"#);
        assert_eq!(wire(&AnyValue::from(1.5)), r#"{"doubleValue":1.5}"#);
        assert_eq!(
            wire(&AnyValue::from(vec![AnyValue::from(1i64)])),
            r#"{"arrayValue":{"values":[{"intValue":"1"}]}}"#
        );
        assert_eq!(
            wire(&AnyValue::from(Vec::<AnyValue>::new())),
            r#"{"arrayValue":{}}"#
        );
        assert_eq!(
            wire(&AnyValue::from(vec![KeyValue::new("k", "v")])),
            r#"{"kvlistValue":{"values":[{"key":"k","value":{"stringValue":"v"}}]}}"#
        );
        assert_eq!(
            wire(&AnyValue::from(vec![1u8, 2, 3])),
            r#"{"bytesValue":"AQID"}"#
        );
    }

    #[test]
    fn every_kind_reads_back() {
        for value in [
            AnyValue::from("s"),
            AnyValue::from(false),
            AnyValue::from(i64::MIN),
            AnyValue::from(0.25),
            AnyValue::from(vec![
                AnyValue::from("a"),
                AnyValue::from(vec![KeyValue::new("k", 1i64)]),
            ]),
            AnyValue::from(Vec::<KeyValue>::new()),
            AnyValue::from(vec![0u8, 255]),
        ] {
            let text = wire(&value);
            assert_eq!(
                serde_json::from_str::<AnyValue>(&text).unwrap(),
                value,
                "{text}"
            );
        }
        assert_eq!(
            serde_json::from_str::<AnyValue>(r#"{"arrayValue":{}}"#).unwrap(),
            AnyValue::Array(vec![])
        );
        assert_eq!(
            serde_json::from_str::<AnyValue>(r#"{"intValue":7}"#).unwrap(),
            AnyValue::Int(7)
        );
        assert!(serde_json::from_str::<AnyValue>("{}").is_err());
        assert!(
            serde_json::from_str::<AnyValue>(r#"{"stringValue":"a","boolValue":true}"#).is_err()
        );
        assert!(serde_json::from_str::<AnyValue>(r#"{"futureValue":1}"#).is_err());
    }

    #[test]
    fn a_pair_without_a_value_is_written_as_its_key() {
        let pair = KeyValue {
            key: "k".into(),
            value: None,
        };
        assert_eq!(serde_json::to_string(&pair).unwrap(), r#"{"key":"k"}"#);
        assert_eq!(
            serde_json::from_str::<KeyValue>(r#"{"key":"k"}"#).unwrap(),
            pair
        );
    }
}
