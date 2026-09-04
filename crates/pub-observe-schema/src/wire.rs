//! The serde helpers of the JSON mapping: 64-bit integers written as decimal strings and read from
//! strings or numbers, enums as integers, base64 bytes, identifiers that may be absent.

use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;

/// Whether a value is its proto3 default and is left out of the encoding.
pub fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// A `u64` (`fixed64`, `uint64`) as a decimal string out, a string or a number in.
pub mod u64_str {
    use super::*;

    /// Write the number as a decimal string.
    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(value)
    }

    /// Read a decimal string or a JSON number.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        deserializer.deserialize_any(U64Visitor)
    }
}

/// An `i64` (`int64`, `sfixed64`) as a decimal string out, a string or a number in.
pub mod i64_str {
    use super::*;

    /// Write the number as a decimal string.
    pub fn serialize<S: Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(value)
    }

    /// Read a decimal string or a JSON number.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
        deserializer.deserialize_any(I64Visitor)
    }
}

/// A list of `u64` (`repeated fixed64`), each as [`u64_str`].
pub mod vec_u64_str {
    use super::*;
    use serde::de::SeqAccess;
    use serde::ser::SerializeSeq;

    /// Write every number as a decimal string.
    pub fn serialize<S: Serializer>(values: &[u64], serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(values.len()))?;
        for value in values {
            seq.serialize_element(&value.to_string())?;
        }
        seq.end()
    }

    /// Read a list of decimal strings or JSON numbers.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u64>, D::Error> {
        struct ListVisitor;
        impl<'de> Visitor<'de> for ListVisitor {
            type Value = Vec<u64>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list of 64-bit integers, as decimal strings or numbers")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u64>, A::Error> {
                let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(value) = seq.next_element::<Elem>()? {
                    out.push(value.0);
                }
                Ok(out)
            }
        }
        struct Elem(u64);
        impl<'de> de::Deserialize<'de> for Elem {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                deserializer.deserialize_any(U64Visitor).map(Elem)
            }
        }
        deserializer.deserialize_seq(ListVisitor)
    }
}

/// `bytes` as standard base64 out, either alphabet with or without padding in.
pub mod base64_bytes {
    use super::*;

    /// Write the bytes as padded standard base64.
    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&crate::base64::encode(bytes))
    }

    /// Read base64 in the standard or the URL-safe alphabet, padded or not.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        struct Base64Visitor;
        impl Visitor<'_> for Base64Visitor {
            type Value = Vec<u8>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("base64 text")
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<Vec<u8>, E> {
                crate::base64::decode(text).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(Base64Visitor)
    }
}

/// An identifier field that may be absent: written only when present, read as absent when the
/// string is empty (the proto3 default of `bytes`).
pub mod opt_id {
    use super::*;
    use serde::Deserialize;

    /// Read an identifier, or nothing from an empty string.
    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: std::str::FromStr<Err = crate::error::Invalid>,
    {
        let text = String::deserialize(deserializer)?;
        if text.is_empty() {
            return Ok(None);
        }
        text.parse()
            .map(Some)
            .map_err(|invalid: crate::error::Invalid| de::Error::custom(invalid.reason))
    }
}

/// The integer-only encoding of an enum: `serialize` writes the number, `deserialize` reads a JSON
/// number and hands it to `from_u32`, refusing a name and a number `from_u32` does not know.
pub fn deserialize_enum<'de, D, T>(
    deserializer: D,
    field: &'static str,
    from_u32: fn(u32) -> Option<T>,
) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
{
    struct EnumVisitor<T> {
        field: &'static str,
        from_u32: fn(u32) -> Option<T>,
    }
    impl<T> Visitor<'_> for EnumVisitor<T> {
        type Value = T;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "an integer value of {} (names are not allowed in OTLP/JSON)",
                self.field
            )
        }

        fn visit_u64<E: de::Error>(self, value: u64) -> Result<T, E> {
            u32::try_from(value)
                .ok()
                .and_then(self.from_u32)
                .ok_or_else(|| E::custom(format!("{} {} is not a known value", self.field, value)))
        }

        fn visit_i64<E: de::Error>(self, value: i64) -> Result<T, E> {
            u64::try_from(value)
                .map_err(|_| E::custom(format!("{} {} is not a known value", self.field, value)))
                .and_then(|value| self.visit_u64(value))
        }
    }
    deserializer.deserialize_any(EnumVisitor { field, from_u32 })
}

struct U64Visitor;

impl Visitor<'_> for U64Visitor {
    type Value = u64;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 64-bit unsigned integer, as a decimal string or a number")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<u64, E> {
        Ok(value)
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<u64, E> {
        u64::try_from(value).map_err(|_| E::custom(format!("{value} is negative")))
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<u64, E> {
        text.parse()
            .map_err(|_| E::custom(format!("{text:?} is not a 64-bit unsigned integer")))
    }
}

struct I64Visitor;

impl Visitor<'_> for I64Visitor {
    type Value = i64;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a 64-bit signed integer, as a decimal string or a number")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<i64, E> {
        i64::try_from(value)
            .map_err(|_| E::custom(format!("{value} exceeds a signed 64-bit integer")))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<i64, E> {
        Ok(value)
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<i64, E> {
        text.parse()
            .map_err(|_| E::custom(format!("{text:?} is not a 64-bit signed integer")))
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Probe {
        #[serde(with = "super::u64_str")]
        big: u64,
        #[serde(with = "super::i64_str")]
        signed: i64,
        #[serde(with = "super::vec_u64_str")]
        list: Vec<u64>,
    }

    #[test]
    fn strings_out_numbers_or_strings_in() {
        let probe = Probe {
            big: u64::MAX,
            signed: i64::MIN,
            list: vec![1, u64::MAX],
        };
        let text = serde_json::to_string(&probe).unwrap();
        assert_eq!(
            text,
            r#"{"big":"18446744073709551615","signed":"-9223372036854775808","list":["1","18446744073709551615"]}"#
        );
        assert_eq!(serde_json::from_str::<Probe>(&text).unwrap(), probe);
        let numbers = r#"{"big":18446744073709551615,"signed":-9223372036854775808,"list":[1,18446744073709551615]}"#;
        assert_eq!(serde_json::from_str::<Probe>(numbers).unwrap(), probe);
        let negative = r#"{"big":-1,"signed":0,"list":[]}"#;
        assert!(
            serde_json::from_str::<Probe>(negative)
                .unwrap_err()
                .to_string()
                .contains("negative")
        );
        let float = r#"{"big":1.5,"signed":0,"list":[]}"#;
        assert!(serde_json::from_str::<Probe>(float).is_err());
        let words = r#"{"big":"many","signed":0,"list":[]}"#;
        assert!(
            serde_json::from_str::<Probe>(words)
                .unwrap_err()
                .to_string()
                .contains("not a 64-bit")
        );
    }
}
