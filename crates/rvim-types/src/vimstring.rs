use std::fmt;
use std::ops::Deref;

/// A byte string with Vim semantics.
///
/// Vim strings are byte arrays and may contain NUL or invalid UTF-8. This type
/// deliberately does not guarantee UTF-8; lossy conversion is explicit.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VimString(Vec<u8>);

impl VimString {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn as_mut_vec(&mut self) -> &mut Vec<u8> {
        &mut self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    pub fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }

    pub fn push(&mut self, byte: u8) {
        self.0.push(byte);
    }

    pub fn push_str(&mut self, s: &str) {
        self.0.extend_from_slice(s.as_bytes());
    }

    pub fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }

    pub fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }

    pub fn slice(&self, start: usize, end: usize) -> &[u8] {
        let start = start.min(self.0.len());
        let end = end.min(self.0.len()).max(start);
        &self.0[start..end]
    }

    pub fn is_utf8(&self) -> bool {
        std::str::from_utf8(&self.0).is_ok()
    }

    /// Returns the first UTF-8 character at `byte`, if the bytes are valid
    /// UTF-8 at that boundary.
    pub fn char_at(&self, byte: usize) -> Option<char> {
        self.0
            .get(byte..)
            .and_then(|rest| std::str::from_utf8(rest).ok()?.chars().next())
    }

    /// Byte index of the next UTF-8 character boundary, or the end.
    pub fn next_boundary(&self, byte: usize) -> usize {
        if byte >= self.0.len() {
            return self.0.len();
        }
        let mut i = byte + 1;
        while i < self.0.len() && (self.0[i] & 0xC0) == 0x80 {
            i += 1;
        }
        i
    }

    /// Byte index of the previous UTF-8 character boundary, or zero.
    pub fn prev_boundary(&self, byte: usize) -> usize {
        if byte == 0 {
            return 0;
        }
        let mut i = byte.min(self.0.len()) - 1;
        while i > 0 && (self.0[i] & 0xC0) == 0x80 {
            i -= 1;
        }
        i
    }
}

impl Deref for VimString {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[u8]> for VimString {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<&str> for VimString {
    fn from(value: &str) -> Self {
        Self(value.as_bytes().to_vec())
    }
}

impl From<String> for VimString {
    fn from(value: String) -> Self {
        Self(value.into_bytes())
    }
}

impl From<Vec<u8>> for VimString {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

impl From<&[u8]> for VimString {
    fn from(value: &[u8]) -> Self {
        Self(value.to_vec())
    }
}

impl From<&VimString> for VimString {
    fn from(value: &VimString) -> Self {
        value.clone()
    }
}

impl fmt::Debug for VimString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", String::from_utf8_lossy(&self.0))
    }
}

impl fmt::Display for VimString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.0))
    }
}
