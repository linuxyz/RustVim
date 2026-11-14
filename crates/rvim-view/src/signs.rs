//! Sign display state.

use std::collections::BTreeMap;

/// One sign as it has to be painted: only the attributes the renderer needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignCell {
    /// Text of the sign (may be empty for a sign that only highlights).
    pub text: String,
    /// Highlight group for the sign text ('texthl').
    pub text_hl: String,
    /// Highlight group for the whole line ('linehl').
    pub line_hl: String,
    /// Highlight group for the line number ('numhl').
    pub num_hl: String,
}

/// Per-window snapshot of the signs displayed on its buffer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignState {
    /// True when the buffer has at least one sign, i.e. 'signcolumn' auto shows
    /// a column.
    pub present: bool,
    /// Signs to show, one per line (the highest priority sign of the line).
    pub cells: BTreeMap<u32, SignCell>,
    /// Buffer the snapshot was built for.
    pub buffer: u64,
    /// Buffer changedtick the snapshot was built from.
    pub changedtick: u64,
    /// Editor-wide sign revision the snapshot was built from.
    pub version: u64,
}

impl SignState {
    pub fn get(&self, lnum: u32) -> Option<&SignCell> {
        self.cells.get(&lnum)
    }
}
