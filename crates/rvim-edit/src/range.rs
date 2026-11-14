use rvim_types::BufPos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeKind {
    Characterwise,
    Linewise,
    Blockwise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextRange {
    pub start: BufPos,
    pub end: BufPos,
    pub kind: RangeKind,
    /// End is inclusive for characterwise motions such as e, f or visual mode.
    pub inclusive: bool,
}

impl TextRange {
    pub fn characterwise(start: BufPos, end: BufPos, inclusive: bool) -> Self {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        Self {
            start,
            end,
            kind: RangeKind::Characterwise,
            inclusive,
        }
    }

    pub fn blockwise(start: BufPos, end: BufPos) -> Self {
        let (first, second) = if start.lnum <= end.lnum {
            (start, end)
        } else {
            (end, start)
        };
        let start_col = first.col.min(second.col);
        let end_col = first.col.max(second.col);
        Self {
            start: BufPos::new(first.lnum, start_col),
            end: BufPos::new(second.lnum, end_col),
            kind: RangeKind::Blockwise,
            inclusive: true,
        }
    }

    pub fn linewise(start_line: u32, end_line: u32) -> Self {
        let (start, end) = if start_line <= end_line {
            (start_line, end_line)
        } else {
            (end_line, start_line)
        };
        Self {
            start: BufPos::new(start, 0),
            end: BufPos::new(end, 0),
            kind: RangeKind::Linewise,
            inclusive: true,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.kind == RangeKind::Characterwise && self.start == self.end && !self.inclusive
    }
}
