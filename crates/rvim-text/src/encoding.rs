//! File encodings and byte-order marks for the UTF-8 internal model.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Latin1,
    Utf16Le,
    Utf16Be,
}

impl Encoding {
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().replace('_', "-").as_str() {
            "utf-8" | "utf8" | "ucs-2" | "ucs2" => Some(Self::Utf8),
            "latin1" | "iso-8859-1" | "iso8859-1" | "l1" => Some(Self::Latin1),
            "utf-16le" | "utf16le" | "ucs-2le" | "ucs2le" => Some(Self::Utf16Le),
            "utf-16be" | "utf16be" | "ucs-2be" | "ucs2be" => Some(Self::Utf16Be),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Latin1 => "latin1",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub text: Vec<u8>,
    pub encoding: Encoding,
    pub bomb: bool,
}

pub fn decode(bytes: &[u8]) -> Decoded {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return Decoded {
            text: rest.to_vec(),
            encoding: Encoding::Utf8,
            bomb: true,
        };
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return Decoded {
            text: decode_utf16(rest, true),
            encoding: Encoding::Utf16Le,
            bomb: true,
        };
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return Decoded {
            text: decode_utf16(rest, false),
            encoding: Encoding::Utf16Be,
            bomb: true,
        };
    }
    if std::str::from_utf8(bytes).is_ok() {
        return Decoded {
            text: bytes.to_vec(),
            encoding: Encoding::Utf8,
            bomb: false,
        };
    }
    Decoded {
        text: decode_latin1(bytes),
        encoding: Encoding::Latin1,
        bomb: false,
    }
}

pub fn decode_with(bytes: &[u8], encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Utf8 => bytes
            .strip_prefix(&[0xEF, 0xBB, 0xBF])
            .unwrap_or(bytes)
            .to_vec(),
        Encoding::Latin1 => decode_latin1(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)),
        Encoding::Utf16Le => decode_utf16(bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes), true),
        Encoding::Utf16Be => {
            decode_utf16(bytes.strip_prefix(&[0xFE, 0xFF]).unwrap_or(bytes), false)
        }
    }
}

pub fn encode(text: &[u8], encoding: Encoding, bomb: bool) -> Vec<u8> {
    let mut out = Vec::new();
    match encoding {
        Encoding::Utf8 => {
            if bomb {
                out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            }
            out.extend_from_slice(text);
        }
        Encoding::Latin1 => {
            for ch in String::from_utf8_lossy(text).chars() {
                let code = ch as u32;
                out.push(if code <= 0xFF { code as u8 } else { b'?' });
            }
        }
        Encoding::Utf16Le => {
            if bomb {
                out.extend_from_slice(&[0xFF, 0xFE]);
            }
            for unit in encode_utf16(text, true) {
                out.extend_from_slice(&unit);
            }
        }
        Encoding::Utf16Be => {
            if bomb {
                out.extend_from_slice(&[0xFE, 0xFF]);
            }
            for unit in encode_utf16(text, false) {
                out.extend_from_slice(&unit);
            }
        }
    }
    out
}

fn decode_latin1(bytes: &[u8]) -> Vec<u8> {
    let mut out = String::with_capacity(bytes.len());
    for byte in bytes {
        out.push(char::from(*byte));
    }
    out.into_bytes()
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Vec<u8> {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units).into_bytes()
}

fn encode_utf16(text: &[u8], little_endian: bool) -> Vec<[u8; 2]> {
    let text = String::from_utf8_lossy(text);
    text.encode_utf16()
        .map(|unit| {
            if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_bom_roundtrip() {
        let bytes = [0xEF, 0xBB, 0xBF, b'h', b'i'];
        let decoded = decode(&bytes);
        assert_eq!(decoded.text, b"hi");
        assert!(decoded.bomb);
        assert_eq!(encode(&decoded.text, Encoding::Utf8, true), bytes);
    }

    #[test]
    fn latin1_fallback() {
        let decoded = decode(&[0xE9]);
        assert_eq!(decoded.encoding, Encoding::Latin1);
        assert_eq!(decoded.text, "é".as_bytes());
    }

    #[test]
    fn utf16_roundtrip() {
        let bytes = [0xFF, 0xFE, b'h', 0, b'i', 0];
        let decoded = decode(&bytes);
        assert_eq!(decoded.text, b"hi");
        assert_eq!(decoded.encoding, Encoding::Utf16Le);
        assert_eq!(encode(&decoded.text, Encoding::Utf16Le, true), bytes);
    }
}
