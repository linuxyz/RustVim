use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Size {
    pub rows: u16,
    pub cols: u16,
}

impl Size {
    pub const fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }

    pub const fn area(self) -> usize {
        self.rows as usize * self.cols as usize
    }

    pub const fn is_empty(self) -> bool {
        self.rows == 0 || self.cols == 0
    }
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.cols, self.rows)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub row: u16,
    pub col: u16,
    pub rows: u16,
    pub cols: u16,
}

impl Rect {
    pub const fn new(row: u16, col: u16, rows: u16, cols: u16) -> Self {
        Self {
            row,
            col,
            rows,
            cols,
        }
    }

    pub const fn from_size(size: Size) -> Self {
        Self {
            row: 0,
            col: 0,
            rows: size.rows,
            cols: size.cols,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.rows == 0 || self.cols == 0
    }

    pub const fn right(self) -> u16 {
        self.col.saturating_add(self.cols)
    }

    pub const fn bottom(self) -> u16 {
        self.row.saturating_add(self.rows)
    }

    pub fn contains(self, row: u16, col: u16) -> bool {
        row >= self.row && row < self.bottom() && col >= self.col && col < self.right()
    }

    pub fn intersect(self, other: Rect) -> Rect {
        let row = self.row.max(other.row);
        let col = self.col.max(other.col);
        let bottom = self.bottom().min(other.bottom());
        let right = self.right().min(other.right());
        Rect {
            row,
            col,
            rows: bottom.saturating_sub(row),
            cols: right.saturating_sub(col),
        }
    }

    pub fn inset(self, top: u16, right: u16, bottom: u16, left: u16) -> Rect {
        let row = self.row.saturating_add(top);
        let col = self.col.saturating_add(left);
        Rect {
            row,
            col,
            rows: self.rows.saturating_sub(top).saturating_sub(bottom),
            cols: self.cols.saturating_sub(left).saturating_sub(right),
        }
    }

    pub fn split_horizontal(self, first_rows: u16) -> (Rect, Rect) {
        let first = Rect {
            rows: first_rows.min(self.rows),
            ..self
        };
        let second = Rect {
            row: self.row.saturating_add(first.rows),
            rows: self.rows.saturating_sub(first.rows),
            ..self
        };
        (first, second)
    }

    pub fn split_vertical(self, first_cols: u16) -> (Rect, Rect) {
        let first = Rect {
            cols: first_cols.min(self.cols),
            ..self
        };
        let second = Rect {
            col: self.col.saturating_add(first.cols),
            cols: self.cols.saturating_sub(first.cols),
            ..self
        };
        (first, second)
    }
}
