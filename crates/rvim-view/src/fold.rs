//! Fold display state and fold-column rendering.
//!
//! The editor computes fold levels, which folds are closed and the cached text
//! of every closed fold; this crate only turns that state into cells. Keeping
//! the data here lets the renderer stay backend-neutral and lets window-local
//! fold state travel with the window (as it does in Vim).

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Fold state of one window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FoldState {
    /// Fold level of every buffer line (index 0 is line 1). 0 means the line is
    /// not inside a fold. Levels are 1-based, like Vim's foldlevel().
    pub levels: Vec<u16>,
    /// For lines inside at least one fold: absolute line number of the
    /// innermost enclosing fold start (0 when the line is not in a fold).
    pub enclosing_start: Vec<u32>,
    /// For a line that starts one or more folds: the lowest fold level that
    /// starts on that line (0 when no fold starts there).
    pub low_level: Vec<u16>,
    /// For a line hidden by a closed fold: start line of the outermost closed
    /// fold that covers it (0 when the line is displayed).
    pub hidden_owner: Vec<u32>,
    /// For a line hidden by a closed fold: last line of that closed fold.
    pub hidden_end: Vec<u32>,
    /// Every fold: (start line, end line inclusive, fold level).
    pub folds: Vec<(u32, u32, u16)>,
    /// Every fold that starts somewhere: start line -> outermost end line.
    pub starts: BTreeMap<u32, u32>,
    /// Closed folds: start line -> end line (inclusive).
    pub closed: BTreeMap<u32, u32>,
    /// Cached fold text for closed folds.
    pub text: HashMap<u32, String>,
    /// Manual folds (start, end inclusive) for 'foldmethod' manual.
    pub manual: Vec<(u32, u32)>,
    /// 'foldmethod' the levels were computed for.
    pub method: String,
    /// 'foldlevel' when the closed set was last derived.
    pub foldlevel: i64,
    /// 'foldenable': when false no fold is treated as closed.
    pub foldenable: bool,
    /// Fold starts explicitly opened with zo (survive recomputation).
    pub opened: BTreeSet<u32>,
    /// Fold starts explicitly closed with zc (survive recomputation).
    pub forced_closed: BTreeSet<u32>,
    /// Buffer the state was computed for.
    pub buffer: u64,
    /// Buffer changedtick the state was computed from.
    pub changedtick: u64,
    /// Fingerprint of the options that produced the levels.
    pub options_key: String,
    /// Set when the editor must recompute (manual fold edits etc.).
    pub dirty: bool,
}

impl FoldState {
    pub fn line_count(&self) -> u32 {
        self.levels.len() as u32
    }

    /// Fold level of \p lnum (0 when outside a fold or out of range).
    pub fn level(&self, lnum: u32) -> u16 {
        self.levels
            .get(lnum.saturating_sub(1) as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Start line of the outermost closed fold covering \p lnum, if any.
    pub fn owner(&self, lnum: u32) -> u32 {
        self.hidden_owner
            .get(lnum.saturating_sub(1) as usize)
            .copied()
            .unwrap_or(0)
    }

    pub fn is_hidden(&self, lnum: u32) -> bool {
        self.owner(lnum) != 0
    }

    /// When \p lnum starts a closed fold, return its last line.
    pub fn closed_start(&self, lnum: u32) -> Option<u32> {
        if self.owner(lnum) == lnum {
            self.hidden_end
                .get(lnum.saturating_sub(1) as usize)
                .copied()
                .filter(|end| *end >= lnum)
        } else {
            None
        }
    }

    /// Outermost closed fold containing \p lnum.
    pub fn closed_containing(&self, lnum: u32) -> Option<(u32, u32)> {
        let owner = self.owner(lnum);
        if owner == 0 {
            return None;
        }
        let end = self
            .hidden_end
            .get(lnum.saturating_sub(1) as usize)
            .copied()
            .unwrap_or(owner);
        Some((owner, end))
    }

    /// End line of the fold starting at \p lnum (open or closed).
    pub fn fold_end(&self, lnum: u32) -> Option<u32> {
        self.starts.get(&lnum).copied()
    }

    /// Render the 'foldcolumn' cells for \p lnum.
    ///
    /// \p closed tells whether the line is displayed as a closed fold. The
    /// algorithm mirrors Vim's fill_foldcolumn(): characters are '|' for open
    /// fold separators, '-' when a fold starts on this line, digits when the
    /// column is too narrow to reach the level, and '+' for a closed fold.
    pub fn fold_column(&self, lnum: u32, closed: bool, fdc: u16, fillchars: &str) -> String {
        if fdc == 0 {
            return String::new();
        }
        let open = fillchars_char(fillchars, "foldopen", '-');
        let close = fillchars_char(fillchars, "foldclose", '+');
        let sep = fillchars_char(fillchars, "foldsep", '|');
        let inner = fillchars_opt(fillchars, "foldinner");
        let level = self.level(lnum);
        let empty: i32 = if fdc == 1 { 0 } else { 1 };
        let first_level =
            (i32::from(level) - i32::from(fdc) - i32::from(closed) + 1 + empty).max(1) as u16;
        let low_level = if closed {
            level
        } else {
            self.low_level
                .get(lnum.saturating_sub(1) as usize)
                .copied()
                .unwrap_or(0)
        };
        let info_lnum = if closed {
            lnum
        } else {
            self.enclosing_start
                .get(lnum.saturating_sub(1) as usize)
                .copied()
                .unwrap_or(0)
        };
        let n = fdc.min(level);
        let mut cells: Vec<char> = Vec::with_capacity(fdc as usize);
        for index in 0..n {
            let this_level = first_level + index;
            let symbol = if info_lnum == lnum && this_level >= low_level {
                open
            } else if first_level == 1 {
                sep
            } else if let Some(inner) = inner {
                inner
            } else if this_level <= 9 {
                char::from(b'0' + this_level as u8)
            } else {
                '>'
            };
            cells.push(symbol);
            if this_level >= level {
                break;
            }
        }
        if closed {
            if let Some(last) = cells.last_mut() {
                *last = close;
            } else {
                cells.push(close);
            }
        }
        while cells.len() < fdc as usize {
            cells.push(' ');
        }
        cells.into_iter().collect()
    }
}

/// One character from 'fillchars', falling back to \p default.
pub fn fillchars_char(fillchars: &str, key: &str, default: char) -> char {
    fillchars_opt(fillchars, key).unwrap_or(default)
}

/// Optional character from 'fillchars'.
pub fn fillchars_opt(fillchars: &str, key: &str) -> Option<char> {
    for entry in fillchars.split(',') {
        let Some((name, value)) = entry.split_once(':') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let unescaped = value.strip_prefix('\\').unwrap_or(value);
        return unescaped.chars().next();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(levels: &[u16], closed: &[(u32, u32)]) -> FoldState {
        let mut state = FoldState {
            levels: levels.to_vec(),
            enclosing_start: vec![0; levels.len()],
            low_level: vec![0; levels.len()],
            hidden_owner: vec![0; levels.len()],
            hidden_end: vec![0; levels.len()],
            ..FoldState::default()
        };
        for (start, end) in closed {
            state.closed.insert(*start, *end);
            for lnum in *start..=*end {
                let index = (lnum - 1) as usize;
                state.hidden_owner[index] = *start;
                state.hidden_end[index] = *end;
            }
        }
        state
    }

    #[test]
    fn fold_column_markers() {
        let fill = "fold:-,foldopen:-,foldclose:+,foldsep:|";
        let mut state = state(&[0, 1, 2, 1, 0], &[(2, 4)]);
        state.enclosing_start = vec![0, 2, 2, 2, 0];
        state.low_level = vec![0, 1, 2, 0, 0];
        // Closed fold start: '+' in the last level column.
        assert_eq!(state.fold_column(2, true, 2, fill), "+ ");
        // Line outside folds: all spaces.
        assert_eq!(state.fold_column(1, false, 2, fill), "  ");
        // Open line inside the fold: separator.
        assert_eq!(state.fold_column(1, false, 2, fill), "  ");
        state.closed.clear();
        state.hidden_owner = vec![0; 5];
        state.hidden_end = vec![0; 5];
        assert_eq!(state.fold_column(2, false, 2, fill), "- ");
        assert_eq!(state.fold_column(3, false, 2, fill), "2 ");
        assert_eq!(state.fold_column(4, false, 2, fill), "| ");
    }
}
