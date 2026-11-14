use std::cmp::Ordering;
use std::fmt;

/// A buffer position.
///
/// `lnum` is 1-based, matching Vim line numbers. `col` is a 0-based byte
/// offset into the line; the public API converts to Vim's 1-based `col()`
/// where needed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BufPos {
    pub lnum: u32,
    pub col: u32,
}

impl BufPos {
    pub const fn new(lnum: u32, col: u32) -> Self {
        Self { lnum, col }
    }

    pub const fn start() -> Self {
        Self { lnum: 1, col: 0 }
    }

    pub fn saturating_add_col(self, n: u32) -> Self {
        Self {
            col: self.col.saturating_add(n),
            ..self
        }
    }
}

impl PartialOrd for BufPos {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BufPos {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.lnum, self.col).cmp(&(other.lnum, other.col))
    }
}

impl fmt::Display for BufPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.lnum, self.col + 1)
    }
}
