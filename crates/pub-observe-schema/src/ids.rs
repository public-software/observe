//! Trace and span identifiers: 16 and 8 bytes, hex on the wire (case-insensitive in, lowercase out).

use std::fmt;
use std::str::FromStr;

use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, Serializer};

use crate::error::Invalid;

macro_rules! id_type {
    ($name:ident, $bytes:literal, $digits:literal, $field:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
        pub struct $name([u8; $bytes]);

        impl $name {
            /// The number of bytes of the identifier.
            pub const LEN: usize = $bytes;

            /// The identifier over these bytes; all zeros is the invalid identifier the protocol
            /// describes and [`is_zero`](Self::is_zero) reports it.
            pub const fn new(bytes: [u8; $bytes]) -> Self {
                $name(bytes)
            }

            /// The identifier written as hex, in either case, exactly the right number of digits.
            pub fn from_hex(text: &str) -> Result<Self, Invalid> {
                let mut bytes = [0u8; $bytes];
                if text.len() != $digits || !text.is_ascii() {
                    return Err(Invalid::new(
                        $field,
                        format!("expected {} hexadecimal digits, got {:?}", $digits, text),
                    ));
                }
                for (i, pair) in text.as_bytes().chunks(2).enumerate() {
                    let hi = hex_digit(pair[0]);
                    let lo = hex_digit(pair[1]);
                    match (hi, lo) {
                        (Some(hi), Some(lo)) => bytes[i] = (hi << 4) | lo,
                        _ => {
                            return Err(Invalid::new(
                                $field,
                                format!("expected {} hexadecimal digits, got {:?}", $digits, text),
                            ));
                        }
                    }
                }
                Ok($name(bytes))
            }

            /// The bytes of the identifier.
            pub const fn as_bytes(&self) -> &[u8; $bytes] {
                &self.0
            }

            /// Whether every byte is zero: the protocol calls such an identifier invalid.
            pub fn is_zero(&self) -> bool {
                self.0.iter().all(|byte| *byte == 0)
            }

            /// The identifier as lowercase hex, the way this crate writes it.
            pub fn to_hex(&self) -> String {
                self.to_string()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                for byte in &self.0 {
                    write!(f, "{byte:02x}")?;
                }
                Ok(())
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self)
            }
        }

        impl FromStr for $name {
            type Err = Invalid;

            fn from_str(text: &str) -> Result<Self, Invalid> {
                Self::from_hex(text)
            }
        }

        impl From<[u8; $bytes]> for $name {
            fn from(bytes: [u8; $bytes]) -> Self {
                $name(bytes)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct HexVisitor;
                impl Visitor<'_> for HexVisitor {
                    type Value = $name;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        write!(f, "{} hexadecimal digits", $digits)
                    }

                    fn visit_str<E: de::Error>(self, text: &str) -> Result<$name, E> {
                        $name::from_hex(text).map_err(|invalid| E::custom(invalid.reason))
                    }
                }
                deserializer.deserialize_str(HexVisitor)
            }
        }
    };
}

id_type!(
    TraceId,
    16,
    32,
    "traceId",
    "The identifier of a trace: 16 bytes, 32 hexadecimal digits on the wire."
);
id_type!(
    SpanId,
    8,
    16,
    "spanId",
    "The identifier of a span within its trace: 8 bytes, 16 hexadecimal digits on the wire."
);

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_and_lowercases() {
        let id = TraceId::from_hex("5B8EFFF798038103D269B633813FC60C").unwrap();
        assert_eq!(id.to_string(), "5b8efff798038103d269b633813fc60c");
        assert_eq!(TraceId::from_hex(&id.to_hex()).unwrap(), id);
        assert_eq!(
            format!("{id:?}"),
            "TraceId(5b8efff798038103d269b633813fc60c)"
        );
        assert_eq!(
            "eee19b7ec3c1b174".parse::<SpanId>().unwrap().as_bytes()[0],
            0xee
        );
    }

    #[test]
    fn refusals_name_the_field_and_the_length() {
        let error = SpanId::from_hex("zz").unwrap_err();
        assert_eq!(error.at, "spanId");
        assert_eq!(error.reason, "expected 16 hexadecimal digits, got \"zz\"");
        let error = TraceId::from_hex("5b8efff798038103d269b633813fc60é").unwrap_err();
        assert!(error.reason.contains("32 hexadecimal digits"));
        assert!(TraceId::from_hex("5b8efff798038103d269b633813fc6 c").is_err());
    }

    #[test]
    fn zero_is_the_invalid_identifier() {
        assert!(TraceId::default().is_zero());
        assert!(SpanId::from([0; 8]).is_zero());
        assert!(!SpanId::from([0, 0, 0, 0, 0, 0, 0, 9]).is_zero());
    }
}
