//! Diff engine and commands (DSP-11/12 core).
//!
//! Two diff windows in the current tab are compared with an internal Myers
//! line diff (the external 'diffexpr' interface is not implemented).  The
//! resulting blocks/filler lines are stored in the view window so the renderer
//! can paint them; 'foldmethod' "diff" derives its fold levels from them.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use rvim_types::{BufPos, BufferId, VimError, VimString, WindowId};
use rvim_view::{DiffBlock, DiffKind, DiffState};

use crate::editor::Editor;

/// Parsed 'diffopt' options that affect the internal diff.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiffOptions {
    pub icase: bool,
    pub iwhite: bool,
    pub iwhiteall: bool,
    pub iwhiteeol: bool,
    pub filler: bool,
    pub context: i64,
}

impl DiffOptions {
    fn parse(diffopt: &str) -> Self {
        let mut options = DiffOptions {
            filler: true,
            ..DiffOptions::default()
        };
        for item in diffopt.split(',') {
            let item = item.trim();
            match item {
                "icase" => options.icase = true,
                "iwhite" => options.iwhite = true,
                "iwhiteall" => options.iwhiteall = true,
                "iwhiteeol" => options.iwhiteeol = true,
                "" => {}
                _ => {
                    if let Some(value) = item.strip_prefix("context:") {
                        options.context = value.parse().unwrap_or(0);
                    }
                    // Other options (internal, algorithm:*, linematch:*, ...)
                    // are accepted by ':set diffopt' but do not change the
                    // internal line diff.
                }
            }
        }
        options
    }

    fn key(&self) -> String {
        format!(
            "icase={} iwhite={} iwhiteall={} iwhiteeol={} filler={} context={}",
            self.icase, self.iwhite, self.iwhiteall, self.iwhiteeol, self.filler, self.context
        )
    }
}

impl Editor {
    /// Recompute the diff state of every window in the current tab.
    pub(crate) fn sync_diffs(&mut self) {
        let windows = self.view.window_ids_in_current_tab();
        let diff_windows: Vec<WindowId> = windows
            .iter()
            .copied()
            .filter(|window| {
                self.options
                    .window_bool(rvim_types::Id::raw(*window), "diff")
            })
            .collect();
        let diffopt = self.options.get_string("diffopt");
        let options = DiffOptions::parse(&diffopt);
        let options_key = options.key();
        for window in windows {
            let active = diff_windows.contains(&window);
            let other = diff_windows
                .iter()
                .copied()
                .find(|candidate| *candidate != window);
            let Some(view_window) = self.view.windows.get(&window) else {
                continue;
            };
            let buffer = view_window.buffer;
            let other_buffer =
                other.and_then(|other| self.view.windows.get(&other).map(|window| window.buffer));
            let tick_self = self
                .buffers
                .get(buffer)
                .map_or(0, |entry| entry.text.meta.changedtick);
            let tick_other = other_buffer
                .and_then(|other| self.buffers.get(other))
                .map_or(0, |entry| entry.text.meta.changedtick);
            let state = &view_window.diff;
            let expected_other = other_buffer.map_or(0, rvim_types::Id::raw);
            let fresh = state.active == active
                && state.other_buffer == expected_other
                && state.tick_self == tick_self
                && state.tick_other == tick_other
                && state.diffopt == options_key;
            if fresh {
                continue;
            }
            let new_state = match other_buffer {
                Some(other_buffer) if active && other_buffer != buffer => {
                    self.compute_diff(buffer, other_buffer, &options, &options_key)
                }
                _ => DiffState::default(),
            };
            let changed = self
                .view
                .windows
                .get(&window)
                .is_some_and(|view_window| view_window.diff != new_state);
            if let Some(view_window) = self.view.window_mut(window) {
                view_window.diff = new_state;
                if changed {
                    view_window.folds.dirty = true;
                }
            }
        }
    }

    fn compute_diff(
        &self,
        buffer: BufferId,
        other_buffer: BufferId,
        options: &DiffOptions,
        options_key: &str,
    ) -> DiffState {
        let Some(entry) = self.buffers.get(buffer) else {
            return DiffState::default();
        };
        let Some(other) = self.buffers.get(other_buffer) else {
            return DiffState::default();
        };
        let tick_self = entry.text.meta.changedtick;
        let tick_other = other.text.meta.changedtick;
        let mut lines: Vec<Vec<u8>> = Vec::with_capacity(entry.text.line_count() as usize);
        for lnum in 1..=entry.text.line_count() {
            lines.push(entry.text.line(lnum).unwrap_or(b"").to_vec());
        }
        let mut other_lines: Vec<Vec<u8>> = Vec::with_capacity(other.text.line_count() as usize);
        for lnum in 1..=other.text.line_count() {
            other_lines.push(other.text.line(lnum).unwrap_or(b"").to_vec());
        }
        let keys: Vec<u64> = lines
            .iter()
            .map(|line| normalize_key(line, options))
            .collect();
        let other_keys: Vec<u64> = other_lines
            .iter()
            .map(|line| normalize_key(line, options))
            .collect();
        let script = myers(&keys, &other_keys);
        build_diff_state(
            &script,
            lines.len(),
            other_lines.len(),
            rvim_types::Id::raw(other_buffer),
            tick_self,
            tick_other,
            options,
            options_key,
        )
    }

    /// 'diff' option changed: force a recompute on the next sync.
    pub(crate) fn diff_changed(&mut self) {
        let windows = self.view.window_ids_in_current_tab();
        for window in windows {
            if let Some(view_window) = self.view.window_mut(window) {
                view_window.diff.tick_self = u64::MAX;
            }
        }
        self.sync_diffs();
    }

    /// 'zo' equivalent for diff windows: ':diffthis' on the current window.
    pub(crate) fn diff_this(&mut self) {
        let window = self.current_window_id();
        let _ = self
            .options
            .set_window(window, "diff", crate::options::OptionValue::Bool(true));
        self.sync_window_options();
    }

    /// ':diffthis' for a specific window id (used by ':diffsplit').
    pub(crate) fn diff_this_on(&mut self, window: u64) {
        let _ = self
            .options
            .set_window(window, "diff", crate::options::OptionValue::Bool(true));
        self.sync_window_options();
    }

    /// ':diffoff' / ':diffoff!'.
    pub(crate) fn diff_off(&mut self, all: bool) {
        if all {
            for window in self.view.window_ids_in_current_tab() {
                let _ = self.options.set_window(
                    rvim_types::Id::raw(window),
                    "diff",
                    crate::options::OptionValue::Bool(false),
                );
            }
        } else {
            let window = self.current_window_id();
            let _ =
                self.options
                    .set_window(window, "diff", crate::options::OptionValue::Bool(false));
        }
        self.sync_window_options();
    }

    /// ':diffupdate'.
    pub(crate) fn diff_update(&mut self) {
        self.diff_changed();
    }

    /// The diff block of the current window containing the cursor.
    fn current_diff_block(
        &mut self,
    ) -> Option<(WindowId, DiffState, DiffBlock, BufferId, BufferId)> {
        self.sync_diffs();
        let window = self.view.current_window()?;
        let buffer = window.buffer;
        if !window.diff.active {
            return None;
        }
        let other = self
            .buffers
            .iter()
            .find(|(raw, _)| *raw == window.diff.other_buffer)
            .map(|(_, entry)| entry.id)?;
        let cursor = self.cursor().lnum;
        let block = window.diff.block_at(cursor).cloned().or_else(|| {
            // A filler-only block is attached to the line after it.
            window
                .diff
                .blocks
                .iter()
                .find(|block| block.filler > 0 && block.start == cursor + 1)
                .cloned()
        })?;
        Some((window.id, window.diff.clone(), block, buffer, other))
    }

    /// 'do' / ':diffget': replace the current changed block with the other
    /// buffer's lines.
    pub(crate) fn diff_get(&mut self) -> Result<(), VimError> {
        let Some((_window, _state, block, buffer, other)) = self.current_diff_block() else {
            return Err(VimError::plain("E99: No diff block under the cursor"));
        };
        if self.current_buffer != buffer {
            return Err(VimError::plain("E99: No diff block under the cursor"));
        }
        let replacement: Vec<VimString> = {
            let other_entry = self
                .buffers
                .get(other)
                .ok_or_else(|| VimError::new(158, "Invalid buffer"))?;
            let start = block.other_start;
            let count = block.other_count;
            (start..start.saturating_add(count))
                .filter_map(|lnum| other_entry.text.line_vim(lnum).cloned())
                .collect()
        };
        self.replace_diff_lines(block.start, block.end, replacement)?;
        self.diff_changed();
        Ok(())
    }

    /// 'dp' / ':diffput': replace the other buffer's block with our lines.
    pub(crate) fn diff_put(&mut self) -> Result<(), VimError> {
        let Some((_window, _state, block, _buffer, other)) = self.current_diff_block() else {
            return Err(VimError::plain("E99: No diff block under the cursor"));
        };
        let lines: Vec<VimString> = {
            let entry = self
                .current_buffer()
                .ok_or_else(|| VimError::new(158, "Invalid buffer"))?;
            let count = block.end.saturating_sub(block.start) + 1;
            (block.start..block.start + count)
                .filter_map(|lnum| entry.text.line_vim(lnum).cloned())
                .collect()
        };
        self.replace_lines_in_buffer(
            other,
            block.other_start,
            block.other_start + block.other_count.saturating_sub(1),
            lines,
        )?;
        self.diff_changed();
        Ok(())
    }

    /// Replace lines in a buffer that is not necessarily the current one.
    fn replace_lines_in_buffer(
        &mut self,
        buffer: BufferId,
        start: u32,
        end: u32,
        replacement: Vec<VimString>,
    ) -> Result<(), VimError> {
        let Some(entry) = self.buffers.get_mut(buffer) else {
            return Err(VimError::new(158, "Invalid buffer"));
        };
        let line_count = entry.text.line_count().max(1);
        let start = start.max(1).min(line_count);
        let end = end.min(line_count);
        entry.undo.record(&entry.text, BufPos::new(start, 0));
        if end >= start {
            entry.text.lines.remove(start - 1, end - start + 1);
        }
        let insert_at = (start - 1).min(entry.text.line_count());
        entry.text.lines.insert_many(insert_at, replacement);
        if entry.text.line_count() == 0 {
            entry.text.lines.insert(0, VimString::new());
        }
        entry.text.touch();
        entry.text.set_modified(true);
        entry.undo.commit(&entry.text, BufPos::new(start, 0));
        self.request_redraw();
        Ok(())
    }

    /// Replace lines start..=end (1-based) with the given lines.
    fn replace_diff_lines(
        &mut self,
        start: u32,
        end: u32,
        replacement: Vec<VimString>,
    ) -> Result<(), VimError> {
        let count = end.saturating_sub(start) + 1;
        let line_count = self.line_count().max(1);
        if start == 0 || start > line_count {
            return Err(VimError::new(16, "Invalid range"));
        }
        self.record_undo();
        {
            let Some(entry) = self.current_buffer_mut() else {
                return Err(VimError::new(158, "Invalid buffer"));
            };
            let end = end.min(line_count);
            if end >= start {
                entry.text.lines.remove(start - 1, end - start + 1);
            }
            let insert_at = (start - 1).min(entry.text.line_count());
            entry.text.lines.insert_many(insert_at, replacement.clone());
            if entry.text.line_count() == 0 {
                entry.text.lines.insert(0, VimString::new());
            }
            entry.text.touch();
        }
        self.commit_undo();
        let _ = count;
        let new_line = start.min(self.line_count().max(1));
        self.set_cursor(BufPos::new(new_line, 0));
        self.request_redraw();
        Ok(())
    }

    /// '[c' / ']c': jump to the previous/next diff change.
    pub(crate) fn diff_jump(&mut self, forward: bool) -> bool {
        self.sync_diffs();
        let Some(window) = self.view.current_window() else {
            return false;
        };
        let cursor = self.cursor().lnum;
        let mut starts: Vec<(u32, u32)> = window
            .diff
            .blocks
            .iter()
            .map(|block| (block.start, block.filler))
            .collect();
        starts.sort_unstable();
        let target = if forward {
            starts.into_iter().find(|(start, _)| *start > cursor)
        } else {
            starts.into_iter().rev().find(|(start, _)| *start < cursor)
        };
        let Some((lnum, _)) = target else {
            return false;
        };
        self.set_cursor(BufPos::new(lnum.max(1), 0));
        self.goto_motion(rvim_edit::Motion::FirstNonBlank);
        true
    }

    /// diff_filler(): filler lines above \p lnum in the current window.
    pub(crate) fn diff_filler_at(&mut self, lnum: i64) -> i64 {
        self.sync_diffs();
        let lnum = if lnum <= 0 {
            self.cursor().lnum
        } else {
            lnum as u32
        };
        self.view
            .current_window()
            .map_or(0, |window| i64::from(window.diff.filler_above(lnum)))
    }

    /// diff_hlID(): pseudo highlight id of the diff highlight at a position.
    pub(crate) fn diff_hl_id(&mut self, lnum: i64, _col: i64) -> i64 {
        self.sync_diffs();
        let lnum = if lnum <= 0 { 0 } else { lnum as u32 };
        let Some(window) = self.view.current_window() else {
            return 0;
        };
        let Some(kind) = window.diff.kind(lnum) else {
            return 0;
        };
        let group = match kind {
            DiffKind::Add => "DiffAdd",
            DiffKind::Change => "DiffChange",
        };
        self.highlight_pseudo_id(group)
    }

    /// Same pseudo id scheme as hlID().
    pub(crate) fn highlight_pseudo_id(&self, name: &str) -> i64 {
        if !self.highlights.contains(name) {
            return 0;
        }
        let mut groups: Vec<&str> = self.highlights.iter().map(|(group, _)| group).collect();
        groups.sort_unstable();
        groups
            .iter()
            .position(|group| *group == name)
            .map_or(0, |position| position as i64 + 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edit {
    Equal,
    Delete,
    Insert,
}

fn normalize_key(line: &[u8], options: &DiffOptions) -> u64 {
    let mut bytes = line.to_vec();
    if options.iwhiteall {
        bytes.retain(|byte| !byte.is_ascii_whitespace());
    } else {
        if options.iwhite {
            let start = bytes.iter().position(|byte| !byte.is_ascii_whitespace());
            let end = bytes.iter().rposition(|byte| !byte.is_ascii_whitespace());
            bytes = match (start, end) {
                (Some(start), Some(end)) => bytes[start..=end].to_vec(),
                _ => Vec::new(),
            };
        } else if options.iwhiteeol {
            while bytes.last().is_some_and(|byte| byte.is_ascii_whitespace()) {
                bytes.pop();
            }
        }
    }
    if options.icase {
        bytes.make_ascii_lowercase();
    }
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Myers diff; returns the edit script from a to b.
fn myers(a: &[u64], b: &[u64]) -> Vec<Edit> {
    let n = a.len();
    let m = b.len();
    if n == 0 && m == 0 {
        return Vec::new();
    }
    if n == 0 {
        return (0..m).map(|_| Edit::Insert).collect();
    }
    if m == 0 {
        return (0..n).map(|_| Edit::Delete).collect();
    }
    let max = n + m;
    let offset = max as isize + 1;
    let size = 2 * max + 3;
    let mut v = vec![0isize; size];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut final_d = 0usize;
    let mut found = false;
    for d in 0..=max {
        trace.push(v.clone());
        let d_i = d as isize;
        let mut k = -d_i;
        while k <= d_i {
            let index = (k + offset) as usize;
            let mut x = if k == -d_i || (k != d_i && v[index - 1] < v[index + 1]) {
                v[index + 1]
            } else {
                v[index - 1] + 1
            };
            let mut y = x - k;
            while x < n as isize && y < m as isize && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[index] = x;
            if x >= n as isize && y >= m as isize {
                final_d = d;
                found = true;
                break;
            }
            k += 2;
        }
        if found {
            break;
        }
    }
    // Backtrack.
    let mut ops: Vec<Edit> = Vec::new();
    let mut x = n as isize;
    let mut y = m as isize;
    for d in (1..=final_d).rev() {
        let v = &trace[d];
        let d_i = d as isize;
        let k = x - y;
        let index = (k + offset) as usize;
        let prev_k = if k == -d_i || (k != d_i && v[index - 1] < v[index + 1]) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = v[(prev_k + offset) as usize];
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            ops.push(Edit::Equal);
            x -= 1;
            y -= 1;
        }
        if x == prev_x {
            ops.push(Edit::Insert);
            y -= 1;
        } else {
            ops.push(Edit::Delete);
            x -= 1;
        }
    }
    while x > 0 && y > 0 {
        ops.push(Edit::Equal);
        x -= 1;
        y -= 1;
    }
    ops.reverse();
    ops
}

#[allow(clippy::too_many_arguments)]
fn build_diff_state(
    script: &[Edit],
    lines_a: usize,
    lines_b: usize,
    other_buffer: u64,
    tick_self: u64,
    tick_other: u64,
    options: &DiffOptions,
    options_key: &str,
) -> DiffState {
    let mut state = DiffState {
        active: true,
        other_buffer,
        tick_self,
        tick_other,
        diffopt: options_key.to_string(),
        line_kinds: vec![None; lines_a],
        filler: vec![0; lines_a],
        blocks: Vec::new(),
    };
    let mut a_index = 0usize;
    let mut b_index = 0usize;
    let mut index = 0usize;
    while index < script.len() {
        match script[index] {
            Edit::Equal => {
                a_index += 1;
                b_index += 1;
                index += 1;
            }
            _ => {
                let a_start = a_index;
                let b_start = b_index;
                let mut a_count = 0u32;
                let mut b_count = 0u32;
                while index < script.len() {
                    match script[index] {
                        Edit::Delete => {
                            a_count += 1;
                            a_index += 1;
                            index += 1;
                        }
                        Edit::Insert => {
                            b_count += 1;
                            b_index += 1;
                            index += 1;
                        }
                        Edit::Equal => break,
                    }
                }
                let start_lnum = a_start as u32 + 1;
                let other_start = b_start as u32 + 1;
                if a_count > 0 {
                    let end_lnum = start_lnum + a_count - 1;
                    let kind = if b_count > 0 {
                        DiffKind::Change
                    } else {
                        DiffKind::Add
                    };
                    for lnum in start_lnum..=end_lnum {
                        if let Some(slot) = state.line_kinds.get_mut((lnum - 1) as usize) {
                            *slot = Some(kind);
                        }
                    }
                    state.blocks.push(DiffBlock {
                        start: start_lnum,
                        end: end_lnum,
                        filler: 0,
                        has_lines: true,
                        other_start,
                        other_count: b_count,
                    });
                } else if b_count > 0 {
                    let filler_line = start_lnum.min(lines_a.max(1) as u32);
                    if options.filler {
                        if let Some(slot) = state.filler.get_mut((filler_line - 1) as usize) {
                            *slot = (*slot).saturating_add(b_count as u16);
                        }
                    }
                    state.blocks.push(DiffBlock {
                        start: filler_line,
                        end: filler_line.saturating_sub(1),
                        filler: b_count,
                        has_lines: false,
                        other_start,
                        other_count: b_count,
                    });
                }
            }
        }
    }
    let _ = lines_b;
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(lines: &[&str]) -> Vec<u64> {
        lines
            .iter()
            .map(|line| normalize_key(line.as_bytes(), &DiffOptions::default()))
            .collect()
    }

    #[test]
    fn myers_identical_and_replacements() {
        let a = keys(&["a", "b", "c"]);
        let script = myers(&a, &a);
        assert_eq!(script.len(), 3);
        assert!(script.iter().all(|edit| matches!(edit, Edit::Equal)));
        let a = keys(&["a", "b", "c"]);
        let b = keys(&["a", "x", "c"]);
        let script = myers(&a, &b);
        let deletes = script
            .iter()
            .filter(|edit| matches!(edit, Edit::Delete))
            .count();
        let inserts = script
            .iter()
            .filter(|edit| matches!(edit, Edit::Insert))
            .count();
        assert_eq!((deletes, inserts), (1, 1));
    }

    #[test]
    fn myers_insert_delete() {
        let a = keys(&["a", "b"]);
        let b = keys(&["a", "b", "c"]);
        let script = myers(&a, &b);
        assert!(script.iter().any(|edit| matches!(edit, Edit::Insert)));
        let script = myers(&b, &a);
        assert!(script.iter().any(|edit| matches!(edit, Edit::Delete)));
    }

    #[test]
    fn diff_options_whitespace_and_case() {
        let options = DiffOptions::parse("icase,iwhite");
        assert_eq!(
            normalize_key(b"  Hello  ", &options),
            normalize_key(b"hello", &options)
        );
        let options = DiffOptions::parse("iwhiteall");
        assert_eq!(
            normalize_key(b"a b c", &options),
            normalize_key(b"abc", &options)
        );
        let options = DiffOptions::parse("iwhiteeol");
        assert_ne!(
            normalize_key(b" a", &options),
            normalize_key(b"a", &options)
        );
        assert_eq!(
            normalize_key(b"a  ", &options),
            normalize_key(b"a", &options)
        );
    }
}
