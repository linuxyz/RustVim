use rvim_types::VimString;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileFormat {
    #[default]
    Unix,
    Dos,
    Mac,
}

impl FileFormat {
    pub fn separator(self) -> &'static [u8] {
        match self {
            Self::Unix => b"\n",
            Self::Dos => b"\r\n",
            Self::Mac => b"\r",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Unix => "unix",
            Self::Dos => "dos",
            Self::Mac => "mac",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "unix" => Some(Self::Unix),
            "dos" => Some(Self::Dos),
            "mac" => Some(Self::Mac),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Detected {
    pub format: FileFormat,
    pub endofline: bool,
    pub bomb: bool,
}

pub fn detect(bytes: &[u8]) -> Detected {
    let bomb = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let body = if bomb { &bytes[3..] } else { bytes };
    let mut format = FileFormat::Unix;
    if let Some(pos) = body.iter().position(|b| *b == b'\n' || *b == b'\r') {
        format = if body[pos] == b'\n' {
            FileFormat::Unix
        } else if body.get(pos + 1) == Some(&b'\n') {
            FileFormat::Dos
        } else {
            FileFormat::Mac
        };
    }
    let endofline = body.is_empty() || body.ends_with(b"\n") || body.ends_with(b"\r");
    Detected {
        format,
        endofline,
        bomb,
    }
}

/// Split file bytes into lines. The line separator is not stored. A single
/// trailing separator means the last line has `endofline`.
pub fn from_bytes(bytes: &[u8], format: FileFormat) -> Vec<VimString> {
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let mut lines: Vec<VimString> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < body.len() {
        let consumed = match format {
            FileFormat::Unix => (body[i] == b'\n').then_some(1),
            FileFormat::Dos => {
                if body[i] == b'\r' && body.get(i + 1) == Some(&b'\n') {
                    Some(2)
                } else if body[i] == b'\n' {
                    Some(1)
                } else {
                    None
                }
            }
            FileFormat::Mac => (body[i] == b'\r').then_some(1),
        };
        if let Some(n) = consumed {
            lines.push(VimString::from(std::mem::take(&mut current)));
            i += n;
        } else {
            current.push(body[i]);
            i += 1;
        }
    }
    if !current.is_empty() {
        lines.push(VimString::from(current));
    } else if lines.is_empty() {
        lines.push(VimString::new());
    }
    lines
}

pub fn to_bytes(lines: &[&[u8]], format: FileFormat, endofline: bool, bomb: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if bomb {
        out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    let sep = format.separator();
    for (index, line) in lines.iter().enumerate() {
        out.extend_from_slice(line);
        if index + 1 < lines.len() || (endofline && !lines.is_empty()) {
            out.extend_from_slice(sep);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_roundtrip() {
        let bytes = b"alpha\nbeta\n";
        let detected = detect(bytes);
        assert_eq!(detected.format, FileFormat::Unix);
        assert!(detected.endofline);
        let lines = from_bytes(bytes, detected.format);
        assert_eq!(lines.len(), 2);
        let refs: Vec<&[u8]> = lines.iter().map(|l| l.as_bytes()).collect();
        assert_eq!(
            to_bytes(&refs, detected.format, detected.endofline, false),
            bytes
        );
    }

    #[test]
    fn dos_detection() {
        let detected = detect(b"a\r\nb\r\n");
        assert_eq!(detected.format, FileFormat::Dos);
        let lines = from_bytes(b"a\r\nb\r\n", detected.format);
        assert_eq!(lines[1].as_bytes(), b"b");
    }
}
