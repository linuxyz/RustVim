//! Byte sets and Vim character classes.
//!
//! Everything here is byte oriented.  Character classes never inspect Unicode
//! properties and case folding is ASCII-only, so arbitrary (including invalid
//! UTF-8) haystacks are safe to use.

/// A set of the 256 possible byte values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ByteSet {
    bits: [u64; 4],
}

impl ByteSet {
    pub(crate) const fn empty() -> Self {
        Self { bits: [0; 4] }
    }

    pub(crate) fn insert(&mut self, byte: u8) {
        let word = usize::from(byte) / 64;
        let bit = usize::from(byte) % 64;
        self.bits[word] |= 1u64 << bit;
    }

    pub(crate) fn insert_range(&mut self, start: u8, end: u8) {
        let mut byte = u16::from(start);
        while byte <= u16::from(end) {
            self.insert(byte as u8);
            byte += 1;
        }
    }

    pub(crate) fn contains(self, byte: u8) -> bool {
        let word = usize::from(byte) / 64;
        let bit = usize::from(byte) % 64;
        self.bits[word] & (1u64 << bit) != 0
    }

    pub(crate) fn union(&mut self, other: &Self) {
        for (dst, src) in self.bits.iter_mut().zip(other.bits.iter()) {
            *dst |= *src;
        }
    }

    pub(crate) fn complement(self) -> Self {
        let mut result = Self::empty();
        for byte in 0u16..=255 {
            if !self.contains(byte as u8) {
                result.insert(byte as u8);
            }
        }
        result
    }
}

/// One compiled character class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CharClass {
    set: ByteSet,
    negated: bool,
    include_newline: bool,
    fold: bool,
    negated_collection: bool,
}

impl CharClass {
    /// Build a `[...]` collection.  Negated collections deliberately match
    /// the line break as well; this is Vim's documented Vi-compatibility quirk.
    pub(crate) fn collection(set: ByteSet, negated: bool, include_newline: bool) -> Self {
        Self {
            set,
            negated,
            include_newline,
            fold: true,
            negated_collection: negated,
        }
    }

    /// Build a class atom such as `\w`.  These do not have the negated
    /// collection line-break quirk.
    pub(crate) fn atom(set: ByteSet, negated: bool, include_newline: bool, fold: bool) -> Self {
        Self {
            set,
            negated,
            include_newline,
            fold,
            negated_collection: false,
        }
    }

    pub(crate) fn matches(self, byte: u8, ignorecase: bool) -> bool {
        if byte == b'\n' {
            return self.include_newline || (self.negated && self.negated_collection);
        }
        let mut hit = self.set.contains(byte);
        if !hit && ignorecase && self.fold && byte.is_ascii_alphabetic() {
            hit = self.set.contains(byte.to_ascii_lowercase())
                || self.set.contains(byte.to_ascii_uppercase());
        }
        if self.negated {
            !hit
        } else {
            hit
        }
    }
}

fn range_set(start: u8, end: u8) -> ByteSet {
    let mut set = ByteSet::empty();
    set.insert_range(start, end);
    set
}

fn alpha() -> ByteSet {
    let mut set = range_set(b'A', b'Z');
    set.insert_range(b'a', b'z');
    set
}

fn digits() -> ByteSet {
    range_set(b'0', b'9')
}

fn head() -> ByteSet {
    let mut set = alpha();
    set.insert(b'_');
    set
}

fn word() -> ByteSet {
    let mut set = head();
    set.insert_range(b'0', b'9');
    set
}

fn whitespace() -> ByteSet {
    let mut set = ByteSet::empty();
    set.insert(b' ');
    set.insert(b'\t');
    set
}

fn hex() -> ByteSet {
    let mut set = digits();
    set.insert_range(b'A', b'F');
    set.insert_range(b'a', b'f');
    set
}

fn octal() -> ByteSet {
    range_set(b'0', b'7')
}

fn printable() -> ByteSet {
    range_set(b' ', b'~')
}

fn fname() -> ByteSet {
    let mut set = word();
    for byte in b"!#$%&'()+,-./:;<=>?@[]^`{|}~\\" {
        set.insert(*byte);
    }
    set
}

fn without_digits(mut set: ByteSet) -> ByteSet {
    for byte in b'0'..=b'9' {
        let word = usize::from(byte) / 64;
        let bit = usize::from(byte) % 64;
        set.bits[word] &= !(1u64 << bit);
    }
    set
}

/// Build a class atom for a letter after a backslash (without the `\_`
/// prefix).  Returns `None` if the letter is not a Vim class atom.
pub(crate) fn class_atom(letter: u8, include_newline: bool) -> Option<CharClass> {
    let (set, negated, fold) = match letter {
        b'd' => (digits(), false, true),
        b'D' => (digits().complement(), false, true),
        b'x' => (hex(), false, true),
        b'X' => (hex().complement(), false, true),
        b'o' => (octal(), false, true),
        b'O' => (octal().complement(), false, true),
        b'w' => (word(), false, true),
        b'W' => (word().complement(), false, true),
        b'h' => (head(), false, true),
        b'H' => (head().complement(), false, true),
        b'a' => (alpha(), false, true),
        b'A' => (alpha().complement(), false, true),
        b'l' => (range_set(b'a', b'z'), false, false),
        b'L' => (range_set(b'a', b'z').complement(), false, false),
        b'u' => (range_set(b'A', b'Z'), false, false),
        b'U' => (range_set(b'A', b'Z').complement(), false, false),
        b's' => (whitespace(), false, true),
        b'S' => (whitespace().complement(), false, true),
        b'i' => (word(), false, true),
        b'I' => (without_digits(word()), false, true),
        b'k' => (word(), false, true),
        b'K' => (without_digits(word()), false, true),
        b'f' => (fname(), false, true),
        b'F' => (without_digits(fname()), false, true),
        b'p' => (printable(), false, true),
        b'P' => (without_digits(printable()), false, true),
        _ => return None,
    };
    Some(CharClass::atom(set, negated, include_newline, fold))
}

/// Look up a POSIX character class by its name (without brackets/colons).
pub(crate) fn posix_class(name: &[u8]) -> Option<ByteSet> {
    let set = match name {
        b"alnum" => {
            let mut set = alpha();
            for byte in b'0'..=b'9' {
                set.insert(byte);
            }
            set
        }
        b"alpha" => alpha(),
        b"blank" => whitespace(),
        b"cntrl" => {
            let mut set = range_set(0, 0x1f);
            set.insert(0x7f);
            set
        }
        b"digit" => digits(),
        b"graph" => range_set(0x21, 0x7e),
        b"lower" => range_set(b'a', b'z'),
        b"print" => printable(),
        b"punct" => {
            let mut set = range_set(0x21, 0x2f);
            set.insert_range(0x3a, 0x40);
            set.insert_range(0x5b, 0x60);
            set.insert_range(0x7b, 0x7e);
            set
        }
        b"space" => {
            let mut set = whitespace();
            set.insert(0x0b);
            set.insert(0x0c);
            set.insert(b'\r');
            set.insert(b'\n');
            set
        }
        b"upper" => range_set(b'A', b'Z'),
        b"xdigit" => hex(),
        b"return" => {
            let mut set = ByteSet::empty();
            set.insert(b'\r');
            set
        }
        b"tab" => {
            let mut set = ByteSet::empty();
            set.insert(b'\t');
            set
        }
        b"escape" => {
            let mut set = ByteSet::empty();
            set.insert(0x1b);
            set
        }
        b"backspace" => {
            let mut set = ByteSet::empty();
            set.insert(0x08);
            set
        }
        b"ident" => word(),
        b"keyword" => word(),
        b"fname" => fname(),
        _ => return None,
    };
    Some(set)
}

/// Vim's keyword test for the fixed ASCII `'iskeyword'` approximation used
/// by this byte-oriented engine.
pub(crate) fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Compare two bytes, folding ASCII case when requested.
pub(crate) fn byte_eq(a: u8, b: u8, ignorecase: bool) -> bool {
    a == b || (ignorecase && a.eq_ignore_ascii_case(&b))
}
