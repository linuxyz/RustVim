//! Diff display state: changed lines, filler rows and diff blocks.

/// How a buffer line participates in a diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffKind {
    /// The line was added in this buffer (DiffAdd).
    Add,
    /// The line differs from the other buffer (DiffChange).
    Change,
}

/// One contiguous diff block in this window's buffer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiffBlock {
    /// First changed line in this buffer (1-based).
    pub start: u32,
    /// Last changed line in this buffer (inclusive; start-1 when the block
    /// only has filler lines).
    pub end: u32,
    /// Number of deleted lines from the other buffer shown as filler rows
    /// above the start line.
    pub filler: u32,
    /// True when this buffer has changed lines in the block.
    pub has_lines: bool,
    /// First line of the corresponding change in the other buffer (1-based).
    pub other_start: u32,
    /// Number of lines in the corresponding change in the other buffer.
    pub other_count: u32,
}

/// Diff state of one window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiffState {
    /// True when the window has 'diff' set and two diff windows were paired.
    pub active: bool,
    /// Buffer of the paired diff window.
    pub other_buffer: u64,
    /// Changedtick of this buffer the blocks were computed from.
    pub tick_self: u64,
    /// Changedtick of the other buffer the blocks were computed from.
    pub tick_other: u64,
    /// Fingerprint of 'diffopt' the blocks were computed from.
    pub diffopt: String,
    /// Highlight kind per line (index 0 is line 1).
    pub line_kinds: Vec<Option<DiffKind>>,
    /// Filler rows displayed above each line (index 0 is line 1).
    pub filler: Vec<u16>,
    /// Contiguous changed blocks.
    pub blocks: Vec<DiffBlock>,
}

impl DiffState {
    pub fn kind(&self, lnum: u32) -> Option<DiffKind> {
        self.line_kinds
            .get(lnum.saturating_sub(1) as usize)
            .copied()
            .flatten()
    }

    pub fn filler_above(&self, lnum: u32) -> u16 {
        self.filler
            .get(lnum.saturating_sub(1) as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Diff block containing a line (a filler-only block counts for the line
    /// it is displayed above).
    pub fn block_at(&self, lnum: u32) -> Option<&DiffBlock> {
        self.blocks
            .iter()
            .find(|block| block.start <= lnum && lnum <= block.end.max(block.start))
    }
}

/// Character used to fill diff filler rows: 'fillchars' "diff", default '-'.
pub fn diff_fill_char(fillchars: &str) -> char {
    for entry in fillchars.split(',') {
        let Some((name, value)) = entry.split_once(':') else {
            continue;
        };
        if name.trim() != "diff" {
            continue;
        }
        let value = value.trim();
        if let Some(ch) = value.strip_prefix('\\').unwrap_or(value).chars().next() {
            return ch;
        }
    }
    '-'
}
