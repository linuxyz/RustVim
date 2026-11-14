use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Color {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AttrFlags(u32);

impl AttrFlags {
    pub const BOLD: Self = Self(1 << 0);
    pub const ITALIC: Self = Self(1 << 1);
    pub const UNDERLINE: Self = Self(1 << 2);
    pub const UNDERCURL: Self = Self(1 << 3);
    pub const UNDERDOUBLE: Self = Self(1 << 4);
    pub const UNDERDOTTED: Self = Self(1 << 5);
    pub const UNDERDASHED: Self = Self(1 << 6);
    pub const STRIKETHROUGH: Self = Self(1 << 7);
    pub const REVERSE: Self = Self(1 << 8);
    pub const BLINK: Self = Self(1 << 9);
    pub const CONCEAL: Self = Self(1 << 10);
    pub const DIM: Self = Self(1 << 11);
    pub const NOCOMBINE: Self = Self(1 << 12);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn from_bits_truncate(bits: u32) -> Self {
        Self(bits & 0x1fff)
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}

impl std::ops::BitOr for AttrFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for AttrFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for AttrFlags {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Attr {
    pub fg: Color,
    pub bg: Color,
    pub sp: Color,
    pub flags: AttrFlags,
}

impl Attr {
    pub const DEFAULT: Self = Self {
        fg: Color::Default,
        bg: Color::Default,
        sp: Color::Default,
        flags: AttrFlags::empty(),
    };

    pub const fn new(fg: Color, bg: Color) -> Self {
        Self {
            fg,
            bg,
            sp: Color::Default,
            flags: AttrFlags::empty(),
        }
    }

    pub const fn with_flags(mut self, flags: AttrFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn is_default(&self) -> bool {
        *self == Self::DEFAULT
    }
}

impl fmt::Display for AttrFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:x}", self.0)
    }
}
