use rvim_types::VimString;

const MAX_CHUNK_LINES: usize = 128;
const MIN_CHUNK_LINES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LineId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub id: LineId,
    pub bytes: VimString,
}

#[derive(Clone, Debug)]
struct Chunk {
    lines: Vec<Line>,
    /// Zero-based global line index of the first line.
    start: u32,
}

/// A chunked line tree with stable line IDs and O(log chunks) access.
#[derive(Clone, Debug)]
pub struct LineTree {
    chunks: Vec<Chunk>,
    total: u32,
    next_id: u64,
}

impl Default for LineTree {
    fn default() -> Self {
        Self::new()
    }
}

impl LineTree {
    pub fn new() -> Self {
        let mut tree = Self {
            chunks: Vec::new(),
            total: 0,
            next_id: 1,
        };
        tree.insert(0, VimString::new());
        tree
    }

    pub fn from_lines(lines: Vec<VimString>) -> Self {
        let mut tree = Self {
            chunks: Vec::new(),
            total: 0,
            next_id: 1,
        };
        if lines.is_empty() {
            tree.insert(0, VimString::new());
            return tree;
        }
        let mut chunk = Chunk {
            lines: Vec::new(),
            start: 0,
        };
        for bytes in lines {
            chunk.lines.push(tree.new_line(bytes));
            if chunk.lines.len() == MAX_CHUNK_LINES {
                tree.chunks.push(chunk);
                let start = tree.total;
                tree.total += MAX_CHUNK_LINES as u32;
                chunk = Chunk {
                    lines: Vec::new(),
                    start,
                };
            }
        }
        if !chunk.lines.is_empty() {
            tree.chunks.push(chunk);
            tree.total += tree.chunks.last().map_or(0, |c| c.lines.len()) as u32;
        }
        tree.reindex();
        tree
    }

    fn new_line(&mut self, bytes: VimString) -> Line {
        let id = LineId(self.next_id);
        self.next_id += 1;
        Line { id, bytes }
    }

    /// Build a tree from existing lines, preserving their stable ids (undo).
    pub fn from_line_vec(lines: Vec<Line>) -> Self {
        let mut tree = Self {
            chunks: Vec::new(),
            total: 0,
            next_id: 1,
        };
        if lines.is_empty() {
            tree.insert(0, VimString::new());
            return tree;
        }
        tree.next_id = lines
            .iter()
            .map(|line| line.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut chunk = Chunk {
            lines: Vec::new(),
            start: 0,
        };
        for line in lines {
            chunk.lines.push(line);
            if chunk.lines.len() == MAX_CHUNK_LINES {
                tree.chunks.push(chunk);
                let start = tree.total;
                tree.total += MAX_CHUNK_LINES as u32;
                chunk = Chunk {
                    lines: Vec::new(),
                    start,
                };
            }
        }
        if !chunk.lines.is_empty() {
            tree.chunks.push(chunk);
        }
        tree.reindex();
        tree
    }

    pub fn line_count(&self) -> u32 {
        self.total
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    pub fn line(&self, lnum: u32) -> Option<&Line> {
        self.line0(lnum.checked_sub(1)?)
    }

    pub fn line0(&self, index: u32) -> Option<&Line> {
        if index >= self.total {
            return None;
        }
        let (chunk, offset) = self.locate(index)?;
        self.chunks[chunk].lines.get(offset)
    }

    pub fn line0_mut(&mut self, index: u32) -> Option<&mut Line> {
        if index >= self.total {
            return None;
        }
        let (chunk, offset) = self.locate(index)?;
        self.chunks[chunk].lines.get_mut(offset)
    }

    pub fn line_by_id(&self, id: LineId) -> Option<&Line> {
        self.chunks
            .iter()
            .flat_map(|c| c.lines.iter())
            .find(|line| line.id == id)
    }

    pub fn set_line(&mut self, lnum: u32, bytes: VimString) {
        if let Some(line) = self.line0_mut(lnum.saturating_sub(1)) {
            line.bytes = bytes;
        }
    }

    pub fn insert(&mut self, index: u32, bytes: VimString) {
        let line = self.new_line(bytes);
        self.insert_line(index, line);
    }

    fn insert_line(&mut self, index: u32, line: Line) {
        if self.total == 0 {
            self.chunks.push(Chunk {
                lines: vec![line],
                start: 0,
            });
            self.total = 1;
            self.reindex();
            return;
        }
        let index = index.min(self.total);
        let (chunk_idx, offset) = self.locate_for_insert(index);
        self.chunks[chunk_idx].lines.insert(offset, line);
        self.total += 1;
        if self.chunks[chunk_idx].lines.len() > MAX_CHUNK_LINES {
            let tail = self.chunks[chunk_idx].lines.split_off(MAX_CHUNK_LINES / 2);
            self.chunks.insert(
                chunk_idx + 1,
                Chunk {
                    lines: tail,
                    start: 0,
                },
            );
        }
        self.reindex();
    }

    pub fn insert_many(&mut self, index: u32, lines: Vec<VimString>) {
        let start = index.min(self.total);
        for (offset, bytes) in lines.into_iter().enumerate() {
            self.insert(start + offset as u32, bytes);
        }
    }

    pub fn remove(&mut self, start: u32, count: u32) -> Vec<Line> {
        if count == 0 || start >= self.total {
            return Vec::new();
        }
        let count = count.min(self.total - start);
        let mut removed = Vec::with_capacity(count as usize);
        let mut remaining = count;
        while remaining > 0 {
            let Some((chunk_idx, offset)) = self.locate(start) else {
                break;
            };
            let available = self.chunks[chunk_idx].lines.len() - offset;
            let take = (available as u32).min(remaining) as usize;
            let mut right = self.chunks[chunk_idx].lines.split_off(offset);
            let tail = right.split_off(take);
            self.chunks[chunk_idx].lines.extend(tail);
            removed.append(&mut right);
            self.total -= take as u32;
            remaining -= take as u32;
            self.reindex();
            if self.chunks[chunk_idx].lines.is_empty() {
                self.chunks.remove(chunk_idx);
            } else if self.chunks[chunk_idx].lines.len() < MIN_CHUNK_LINES && self.chunks.len() > 1
            {
                self.merge_into_neighbor(chunk_idx);
            }
        }
        self.reindex();
        removed
    }

    fn merge_into_neighbor(&mut self, chunk_idx: usize) {
        if chunk_idx + 1 < self.chunks.len() {
            let next = self.chunks.remove(chunk_idx + 1);
            self.chunks[chunk_idx].lines.extend(next.lines);
        } else if chunk_idx > 0 {
            let current = self.chunks.remove(chunk_idx);
            self.chunks[chunk_idx - 1].lines.extend(current.lines);
        }
    }

    fn locate(&self, index: u32) -> Option<(usize, usize)> {
        if index >= self.total {
            return None;
        }
        let chunk_idx = self.locate_chunk(index);
        let start = self.chunks.get(chunk_idx)?.start;
        let offset = (index - start) as usize;
        if offset < self.chunks.get(chunk_idx)?.lines.len() {
            Some((chunk_idx, offset))
        } else {
            None
        }
    }

    fn locate_for_insert(&self, index: u32) -> (usize, usize) {
        if self.chunks.is_empty() {
            return (0, 0);
        }
        if index >= self.total {
            let last = self.chunks.len() - 1;
            return (last, self.chunks[last].lines.len());
        }
        let chunk_idx = self.locate_chunk(index);
        let offset = (index - self.chunks[chunk_idx].start) as usize;
        (chunk_idx, offset)
    }

    fn locate_chunk(&self, index: u32) -> usize {
        let mut lo = 0usize;
        let mut hi = self.chunks.len();
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if self.chunks[mid].start <= index {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    fn reindex(&mut self) {
        let mut start = 0u32;
        for chunk in &mut self.chunks {
            chunk.start = start;
            start += chunk.lines.len() as u32;
        }
        self.total = start;
    }

    pub fn iter(&self) -> impl Iterator<Item = &Line> + '_ {
        self.chunks.iter().flat_map(|chunk| chunk.lines.iter())
    }

    pub fn chunks_len(&self) -> usize {
        self.chunks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &LineTree, lnum: u32) -> String {
        String::from_utf8_lossy(line.line(lnum).unwrap().bytes.as_bytes()).into_owned()
    }

    #[test]
    fn insert_delete_across_chunks() {
        let mut tree = LineTree::new();
        for i in 0..1000 {
            tree.insert(i, VimString::from(format!("line {i}")));
        }
        assert_eq!(tree.line_count(), 1001);
        assert_eq!(text(&tree, 1), "line 0");
        assert!(tree.chunks_len() > 1);
        tree.remove(100, 500);
        assert_eq!(tree.line_count(), 501);
        assert_eq!(text(&tree, 1), "line 0");
        assert_eq!(text(&tree, 100), "line 99");
        assert_eq!(text(&tree, 101), "line 600");
        tree.insert(100, VimString::from("inserted"));
        assert_eq!(text(&tree, 101), "inserted");
    }
}
