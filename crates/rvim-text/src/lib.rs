//! Byte-oriented text storage and undo for RVim.
//!
//! Buffers are line-oriented. A line never contains a newline byte; the file
//! format layer owns line separators. Lines are stored in small chunks so that
//! insertion and deletion are O(log n) in the number of chunks while line
//! access remains cache-friendly.

pub mod encoding;
pub mod fileio;
pub mod line_tree;
pub mod undo;

pub use encoding::{decode, decode_with, encode, Decoded, Encoding};
pub use fileio::{detect, from_bytes, to_bytes, FileFormat};
pub use line_tree::{Line, LineId, LineTree};
pub use undo::{Snapshot, UndoManager, UndoNode};

use rvim_types::VimString;

/// Metadata that belongs to the text storage rather than the presentation layer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextMeta {
    pub name: Option<std::path::PathBuf>,
    pub changedtick: u64,
    pub modified: bool,
    pub fileformat: FileFormat,
    pub endofline: bool,
    pub bomb: bool,
    pub fileencoding: String,
    pub modeline: bool,
    pub modelines: i64,
}

impl TextMeta {
    pub fn new() -> Self {
        Self {
            name: None,
            changedtick: 0,
            modified: false,
            fileformat: FileFormat::Unix,
            endofline: true,
            bomb: false,
            fileencoding: "utf-8".to_string(),
            modeline: true,
            modelines: 5,
        }
    }
}

/// A text buffer: line tree plus file metadata. It does not own a cursor or a
/// viewport; those belong to a window.
#[derive(Clone, Debug)]
pub struct TextBuffer {
    pub lines: LineTree,
    pub meta: TextMeta,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            lines: LineTree::new(),
            meta: TextMeta::new(),
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let detected = detect(bytes);
        let lines = from_bytes(bytes, detected.format);
        let mut buffer = Self::new();
        buffer.lines = LineTree::from_lines(lines);
        buffer.meta.fileformat = detected.format;
        buffer.meta.endofline = detected.endofline;
        buffer.meta.bomb = detected.bomb;
        buffer.meta.changedtick = 1;
        buffer.meta.modified = false;
        buffer
    }

    pub fn line_count(&self) -> u32 {
        self.lines.line_count()
    }

    /// 1-based line access, Vim style.
    pub fn line(&self, lnum: u32) -> Option<&[u8]> {
        self.lines.line(lnum).map(|line| line.bytes.as_bytes())
    }

    pub fn line_vim(&self, lnum: u32) -> Option<&VimString> {
        self.lines.line(lnum).map(|line| &line.bytes)
    }

    pub fn line_len(&self, lnum: u32) -> u32 {
        self.line(lnum).map_or(0, |line| line.len() as u32)
    }

    pub fn replace_line(&mut self, lnum: u32, bytes: impl Into<VimString>) -> bool {
        if lnum == 0 || lnum > self.line_count() {
            return false;
        }
        self.lines.set_line(lnum, bytes.into());
        self.touch();
        true
    }

    pub fn insert_line(&mut self, before: u32, bytes: impl Into<VimString>) {
        let index = before.saturating_sub(1).min(self.line_count());
        self.lines.insert(index, bytes.into());
        self.touch();
    }

    pub fn delete_line(&mut self, lnum: u32) -> bool {
        if lnum == 0 || lnum > self.line_count() {
            return false;
        }
        self.lines.remove(lnum - 1, 1);
        if self.line_count() == 0 {
            self.lines.insert(0, VimString::new());
        }
        self.touch();
        true
    }

    /// Insert bytes at a byte offset in a line, splitting when the bytes contain
    /// newlines. Returns the resulting cursor position (end of inserted text).
    pub fn insert_bytes(&mut self, lnum: u32, col: u32, bytes: &[u8]) -> rvim_types::BufPos {
        let lnum = lnum.max(1).min(self.line_count().max(1));
        let current = self.line(lnum).unwrap_or(b"").to_vec();
        let col = (col as usize).min(current.len());
        if !bytes.contains(&b'\n') && !bytes.contains(&b'\r') {
            let mut next = Vec::with_capacity(current.len() + bytes.len());
            next.extend_from_slice(&current[..col]);
            next.extend_from_slice(bytes);
            next.extend_from_slice(&current[col..]);
            self.lines.set_line(lnum, VimString::from(next));
            self.touch();
            return rvim_types::BufPos::new(lnum, (col + bytes.len()) as u32);
        }
        let mut combined = Vec::with_capacity(current.len() + bytes.len());
        combined.extend_from_slice(&current[..col]);
        combined.extend_from_slice(bytes);
        combined.extend_from_slice(&current[col..]);
        let new_lines = split_lines_any(&combined);
        let end_lnum = lnum + new_lines.len() as u32 - 1;
        let end_col = new_lines.last().map_or(0, |line| line.len()) as u32;
        self.lines.remove(lnum - 1, 1);
        self.lines.insert_many(
            lnum - 1,
            new_lines.into_iter().map(VimString::from).collect(),
        );
        self.touch();
        rvim_types::BufPos::new(end_lnum, end_col)
    }

    /// Delete a byte range. The end position is exclusive. If the range crosses
    /// lines, the lines are joined.
    pub fn delete_range(&mut self, start: rvim_types::BufPos, end: rvim_types::BufPos) -> bool {
        if end < start || start.lnum == 0 || end.lnum > self.line_count() {
            return false;
        }
        if start == end {
            return false;
        }
        if start.lnum == end.lnum {
            let line = self.line(start.lnum).unwrap_or(b"").to_vec();
            let start_col = (start.col as usize).min(line.len());
            let end_col = (end.col as usize).min(line.len()).max(start_col);
            let mut next = Vec::with_capacity(line.len() - (end_col - start_col));
            next.extend_from_slice(&line[..start_col]);
            next.extend_from_slice(&line[end_col..]);
            self.lines.set_line(start.lnum, VimString::from(next));
            self.touch();
            return true;
        }
        let first = self.line(start.lnum).unwrap_or(b"").to_vec();
        let last = self.line(end.lnum).unwrap_or(b"").to_vec();
        let start_col = (start.col as usize).min(first.len());
        let end_col = (end.col as usize).min(last.len());
        let mut joined = Vec::with_capacity(first.len() + last.len());
        joined.extend_from_slice(&first[..start_col]);
        joined.extend_from_slice(&last[end_col..]);
        self.lines.remove(start.lnum - 1, end.lnum - start.lnum + 1);
        self.lines.insert(start.lnum - 1, VimString::from(joined));
        self.touch();
        true
    }

    /// Replace the whole buffer with bytes from a file.
    pub fn load_from_bytes(&mut self, bytes: &[u8]) {
        let detected = detect(bytes);
        let lines = from_bytes(bytes, detected.format);
        self.lines = LineTree::from_lines(lines);
        self.meta.fileformat = detected.format;
        self.meta.endofline = detected.endofline;
        self.meta.bomb = detected.bomb;
        self.meta.modified = false;
        self.meta.changedtick = self.meta.changedtick.wrapping_add(1);
    }

    pub fn to_file_bytes_range(&self, start: u32, end: u32) -> Vec<u8> {
        let start = start.max(1);
        let end = end.max(start).min(self.lines.line_count());
        let lines: Vec<&[u8]> = (start..=end)
            .filter_map(|lnum| self.lines.line(lnum))
            .map(|line| line.bytes.as_bytes())
            .collect();
        to_bytes(&lines, self.meta.fileformat, true, self.meta.bomb)
    }

    pub fn to_file_bytes(&self) -> Vec<u8> {
        let lines: Vec<&[u8]> = self
            .lines
            .iter()
            .map(|line| line.bytes.as_bytes())
            .collect();
        to_bytes(
            &lines,
            self.meta.fileformat,
            self.meta.endofline,
            self.meta.bomb,
        )
    }

    pub fn touch(&mut self) {
        self.meta.changedtick = self.meta.changedtick.wrapping_add(1);
        self.meta.modified = true;
    }

    pub fn set_modified(&mut self, modified: bool) {
        self.meta.modified = modified;
    }

    pub fn lines_vec(&self) -> Vec<VimString> {
        self.lines.iter().map(|line| line.bytes.clone()).collect()
    }

    /// Lines with their stable ids, for undo snapshots.
    pub fn snapshot_lines(&self) -> Vec<Line> {
        self.lines.iter().cloned().collect()
    }

    /// Restore a snapshot, keeping line ids so signs keep their lines.
    pub fn restore_snapshot(&mut self, lines: &[Line]) {
        self.lines = LineTree::from_line_vec(lines.to_vec());
        self.meta.changedtick = self.meta.changedtick.wrapping_add(1);
    }

    pub fn restore_lines(&mut self, lines: Vec<VimString>) {
        self.lines = LineTree::from_lines(lines);
        self.meta.changedtick = self.meta.changedtick.wrapping_add(1);
    }
}

fn split_lines_any(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                out.push(std::mem::take(&mut current));
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
            }
            b'\n' => out.push(std::mem::take(&mut current)),
            byte => current.push(byte),
        }
        i += 1;
    }
    out.push(current);
    out
}
