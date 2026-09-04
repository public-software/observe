//! Standard base64 (RFC 4648 §4) for `bytesValue`: written with padding, read from either alphabet
//! with or without padding, the leniency the Protobuf JSON mapping grants a parser.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode `bytes` in the standard alphabet with `=` padding.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut word = 0u32;
        for (i, byte) in chunk.iter().enumerate() {
            word |= u32::from(*byte) << (16 - 8 * i);
        }
        let symbols = chunk.len() + 1;
        for i in 0..4 {
            if i < symbols {
                let index = (word >> (18 - 6 * i)) & 0x3f;
                out.push(ALPHABET[index as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decode text in the standard or the URL-safe alphabet, padded or not.
///
/// Refuses a symbol outside both alphabets, padding in the wrong place and a length that no byte
/// string encodes to.
pub fn decode(text: &str) -> Result<Vec<u8>, String> {
    let trimmed = text.trim_end_matches('=');
    let padding = text.len() - trimmed.len();
    if padding > 2 || (padding > 0 && !text.len().is_multiple_of(4)) {
        return Err(format!("not base64: misplaced padding in {text:?}"));
    }
    if trimmed.len() % 4 == 1 {
        return Err(format!(
            "not base64: {} symbols encode no byte string",
            trimmed.len()
        ));
    }
    let mut out = Vec::with_capacity(trimmed.len() * 3 / 4);
    let mut word = 0u32;
    let mut bits = 0u32;
    for byte in trimmed.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => {
                return Err(format!(
                    "not base64: the symbol {:?} in {text:?}",
                    byte as char
                ));
            }
        };
        word = (word << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((word >> bits) as u8);
            word &= (1 << bits) - 1;
        }
    }
    if word != 0 {
        return Err(format!("not base64: non-zero trailing bits in {text:?}"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rfc_4648_vectors() {
        // the seven vectors of RFC 4648 §10: the prefixes of "foobar"
        let encodings = [
            "", "Zg==", "Zm8=", "Zm9v", "Zm9vYg==", "Zm9vYmE=", "Zm9vYmFy",
        ];
        for (length, encoded) in encodings.into_iter().enumerate() {
            let plain = &"foobar"[..length];
            assert_eq!(encode(plain.as_bytes()), encoded);
            assert_eq!(decode(encoded).unwrap(), plain.as_bytes());
            assert_eq!(
                decode(encoded.trim_end_matches('=')).unwrap(),
                plain.as_bytes(),
                "unpadded {encoded}"
            );
        }
    }

    #[test]
    fn both_alphabets_and_the_refusals() {
        assert_eq!(encode(&[0xfb, 0xff]), "+/8=");
        assert_eq!(decode("-_8=").unwrap(), [0xfb, 0xff]);
        assert_eq!(decode("+/8").unwrap(), [0xfb, 0xff]);
        assert!(decode("Zg=").unwrap_err().contains("padding"));
        assert!(decode("Zg===").unwrap_err().contains("padding"));
        assert!(decode("Z").unwrap_err().contains("encode no byte string"));
        assert!(decode("Zg?=").unwrap_err().contains("symbol"));
        assert!(decode("Zh==").unwrap_err().contains("trailing bits"));
    }
}
